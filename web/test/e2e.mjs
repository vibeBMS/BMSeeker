// End to end, in Chromium (Playwright): BMSeeker web installs a song from a
// link into the vibeBMS library in the page's storage (OPFS bms/), through
// a CORS proxy, and an open vibeBMS page shows it without a reload (the
// BroadcastChannel "vibebms-library").
//
//   node web/test/e2e.mjs <BMSeeker site> [<vibeBMS web folder>] [<screenshots>]
//
// One local server stands for GitHub Pages (/BMSeeker/, /vibe/), the proxy
// (/proxy/?url=) and the host the song is on (/files/song.zip). Without a
// vibeBMS folder, only BMSeeker's side is checked.
import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { chromium } = require("playwright");
const [site, vibeDir, shots = "."] = process.argv.slice(2);
if (!site) throw new Error("usage: e2e.mjs <site> [<vibe web>] [<screenshots>]");
fs.mkdirSync(shots, { recursive: true });

// a song: a chart and its sound, in a zip (stored)
function zip(files) {
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc = (b) => {
    let c = 0xffffffff;
    for (const x of b) c = crcTable[(c ^ x) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const local = [], central = [];
  let at = 0;
  for (const [name, data] of files) {
    const n = Buffer.from(name), d = Buffer.from(data), c = crc(d);
    const h = Buffer.alloc(30);
    h.writeUInt32LE(0x04034b50, 0); h.writeUInt16LE(20, 4); h.writeUInt16LE(0x800, 6);
    h.writeUInt32LE(c, 14); h.writeUInt32LE(d.length, 18); h.writeUInt32LE(d.length, 22);
    h.writeUInt16LE(n.length, 26);
    const e = Buffer.alloc(46);
    e.writeUInt32LE(0x02014b50, 0); e.writeUInt16LE(20, 4); e.writeUInt16LE(20, 6); e.writeUInt16LE(0x800, 8);
    e.writeUInt32LE(c, 16); e.writeUInt32LE(d.length, 20); e.writeUInt32LE(d.length, 24);
    e.writeUInt16LE(n.length, 28); e.writeUInt32LE(at, 42);
    local.push(h, n, d);
    central.push(e, n);
    at += h.length + n.length + d.length;
  }
  const cd = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0); end.writeUInt16LE(files.length, 8); end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(cd.length, 12); end.writeUInt32LE(at, 16);
  return Buffer.concat([...local, cd, end]);
}
const wav = (() => {
  const b = Buffer.alloc(44 + 800);
  b.write("RIFF", 0); b.writeUInt32LE(36 + 800, 4); b.write("WAVEfmt ", 8); b.writeUInt32LE(16, 16);
  b.writeUInt16LE(1, 20); b.writeUInt16LE(1, 22); b.writeUInt32LE(8000, 24); b.writeUInt32LE(16000, 28);
  b.writeUInt16LE(2, 32); b.writeUInt16LE(16, 34); b.write("data", 36); b.writeUInt32LE(800, 40);
  return b;
})();
const chart = "#PLAYER 1\r\n#TITLE Seeker Test Song\r\n#ARTIST e2e\r\n#BPM 120\r\n#PLAYLEVEL 1\r\n#WAV01 01.wav\r\n#00111:01010101\r\n";
const song = zip([["Seeker Test Song/test.bms", chart], ["Seeker Test Song/01.wav", wav]]);

const types = { ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript", ".wasm": "application/wasm",
  ".json": "application/json", ".svg": "image/svg+xml", ".png": "image/png", ".webmanifest": "application/manifest+json",
  ".otf": "font/otf", ".txt": "text/plain" };
const proxied = [];
const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, "http://x");
  const p = decodeURIComponent(url.pathname);
  if (p === "/files/song.zip") {
    res.writeHead(200, { "Content-Type": "application/zip", "Content-Disposition": 'attachment; filename="Seeker Test Song.zip"' });
    return res.end(song);
  }
  if (p === "/proxy/") {
    // as vibeBMS/proxy's worker.js: the body, where the redirects ended
    const target = url.searchParams.get("url");
    proxied.push(target);
    const up = await fetch(target);
    const body = Buffer.from(await up.arrayBuffer());
    const h = { "Access-Control-Allow-Origin": "*", "Access-Control-Expose-Headers": "Content-Length, X-Final-Url", "X-Final-Url": up.url };
    for (const k of ["content-type", "content-disposition"]) if (up.headers.get(k)) h[k] = up.headers.get(k);
    res.writeHead(up.status, h);
    return res.end(body);
  }
  const roots = [["/BMSeeker/", site], ["/vibe/", vibeDir]];
  for (const [prefix, dir] of roots) {
    if (!dir || !p.startsWith(prefix)) continue;
    let f = path.join(dir, p.slice(prefix.length));
    if (!f.startsWith(path.resolve(dir))) break;
    if (fs.existsSync(f) && fs.statSync(f).isDirectory()) f = path.join(f, "index.html");
    if (!fs.existsSync(f)) break;
    const body = fs.readFileSync(f);
    const gz = /gzip/.test(req.headers["accept-encoding"] || "") && /\.(wasm|js)$/.test(f);
    res.writeHead(200, { "Content-Type": types[path.extname(f)] || "application/octet-stream", ...(gz ? { "Content-Encoding": "gzip" } : {}) });
    return res.end(gz ? zlib.gzipSync(body) : body);
  }
  res.writeHead(404);
  res.end("not found");
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const base = `http://localhost:${server.address().port}`;

// The window is drawn on a canvas: its controls are clicked where they are
// drawn at 1100 x 760.
const AT = {
  link: [110, 60], settings: [255, 60],
  url: [550, 126], title: [278, 165], into: [550, 203],
  zip: [20, 170], download: [1010, 227],
};

const browser = await chromium.launch({ args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"] });
const ctx = await browser.newContext({ viewport: { width: 1100, height: 760 }, acceptDownloads: true });
const logs = { seeker: [], vibe: [] };
const step = (s) => console.log(`- ${s}`);
let failed = false;
const check = (ok, what) => {
  console.log(`${ok ? "ok" : "FAILED"}: ${what}`);
  failed ||= !ok;
};

try {
  const seeker = await ctx.newPage();
  seeker.on("console", (m) => logs.seeker.push(m.text()));
  seeker.on("pageerror", (e) => logs.seeker.push("pageerror: " + e.message));
  await seeker.goto(`${base}/BMSeeker/?proxy=${encodeURIComponent(base + "/proxy/")}`);
  await seeker.waitForFunction(() => globalThis.bmseeker && bmseeker.state().ready, null, { timeout: 60000 });
  check(await seeker.evaluate(() => bmseeker.state().kind === "vibe" && bmseeker.state().library === "/bms"), "BMSeeker opens the vibeBMS library");
  await seeker.waitForTimeout(1500);
  await seeker.screenshot({ path: `${shots}/1-bmseeker.png` });

  // the settings and the licences need no target: a worker asked before
  // one is opened (as a folder waiting for the user's permission leaves it)
  // answers
  const early = await seeker.evaluate(async (proxy) => {
    const src = [...document.scripts].find((s) => /bmseeker\.js$/.test(s.src)).src;
    const w = new Worker(new URL("worker.js", src), { type: "module" });
    const ask = (id, args) =>
      new Promise((resolve) => {
        const records = [];
        w.onmessage = (e) => {
          if (e.data.kind === "record") records.push(JSON.parse(e.data.line));
          else if (e.data.ok !== undefined) resolve({ ...e.data, records });
        };
        w.postMessage({ id, op: "run", args, stdin: "", proxy });
      });
    const settings = await ask(1, ["--json", "config", "show"]);
    const licences = await ask(2, ["--json", "licenses"]);
    w.terminate();
    const config = settings.records.find((r) => r.config);
    const text = (licences.records.find((r) => r.licenses) || {}).text || "";
    return {
      ok: settings.ok, error: settings.error, tables: config ? config.config.tables.length : 0,
      licences: { ok: licences.ok, code: licences.value && licences.value.code, error: licences.error, bytes: text.length,
        mit: text.startsWith("MIT License\n"), crate: text.includes("\n- `serde` ("), module: text.includes("\n- `github.com/egoist/mygo`: ") },
    };
  }, `${base}/proxy/`);
  check(early.ok && early.tables > 0, `the settings are read before any target is opened (${JSON.stringify(early)})`);
  const l = early.licences;
  check(l.ok && l.code === 0 && l.mit && l.crate && l.module, `the licences are read before any target is opened (${JSON.stringify(l)})`);
  // and through the page's own run, as the window's Settings asks for them
  const shown = await seeker.evaluate(async () => {
    let text = "";
    const code = await bmseeker.run(["--json", "licenses"], (kind, line) => {
      if (kind === "record") text = JSON.parse(line).text || text;
    });
    return { code, bytes: text.length, mit: text.startsWith("MIT License\n") };
  });
  check(shown.code === 0 && shown.mit && shown.bytes > 300000, `the page's run gives the licences (${JSON.stringify(shown)})`);

  let vibe = null;
  if (vibeDir) {
    vibe = await ctx.newPage();
    vibe.on("console", (m) => logs.vibe.push(m.text()));
    vibe.on("pageerror", (e) => logs.vibe.push("pageerror: " + e.message));
    await vibe.goto(`${base}/vibe/`);
    await vibe.waitForFunction(() => !document.getElementById("msg") || document.getElementById("msg").style.display === "none", null, { timeout: 120000 });
    await vibe.waitForTimeout(4000);
    await vibe.screenshot({ path: `${shots}/2-vibe-before.png` });
    step("vibeBMS open, its library read");
  }

  // the GUI's Link tab: the link, the title, "Into the library"
  await seeker.bringToFront();
  const link = `${base}/files/song.zip`;
  await install(seeker, link, `${shots}/2-bmseeker-link.png`);
  const inOpfs = async () =>
    seeker.evaluate(async () => {
      const out = [];
      const walk = async (d, prefix) => {
        for await (const [n, h] of d.entries()) {
          if (h.kind === "directory") await walk(h, prefix + n + "/");
          else out.push(prefix + n);
        }
      };
      try {
        await walk(await (await navigator.storage.getDirectory()).getDirectoryHandle("bms"), "bms/");
      } catch (e) {}
      return out;
    });
  for (let i = 0; i < 60 && !(await inOpfs()).some((f) => f.endsWith("test.bms")); i++) await seeker.waitForTimeout(500);
  const files = await inOpfs();
  check(files.some((f) => f.endsWith("/test.bms")) && files.some((f) => f.endsWith("/01.wav")), `the song is in OPFS bms/ (${files.join(", ")})`);
  check(proxied.includes(link), "the download went through the proxy");
  await seeker.waitForTimeout(1000);
  await seeker.screenshot({ path: `${shots}/3-bmseeker-installed.png` });

  if (vibe) {
    await vibe.bringToFront();
    const seen = () => logs.vibe.some((l) => /Library: [1-9]\d* charts/.test(l));
    for (let i = 0; i < 60 && !seen(); i++) await vibe.waitForTimeout(500);
    check(logs.vibe.some((l) => l.includes("changed by BMSeeker")), "vibeBMS heard of the change (BroadcastChannel)");
    check(seen(), "vibeBMS read the new chart without a reload");
    await vibe.waitForTimeout(1500);
    await vibe.screenshot({ path: `${shots}/4-vibe-after.png` });
  }

  // the zip target: chosen in Settings, the song in it, downloaded
  await seeker.bringToFront();
  await seeker.mouse.click(...AT.settings);
  await seeker.waitForTimeout(400);
  await seeker.mouse.click(...AT.zip);
  await seeker.waitForFunction(() => bmseeker.state().kind === "zip" && bmseeker.state().ready, null, { timeout: 20000 });
  // (a click as the window takes the new target in may be lost)
  await seeker.waitForTimeout(1500);
  await install(seeker, link, `${shots}/6a-zip-link.png`);
  await seeker.waitForFunction(() => bmseeker.state().files > 0, null, { timeout: 30000 }).catch(async (e) => {
    await seeker.screenshot({ path: `${shots}/6b-zip-failed.png` });
    throw e;
  });
  await seeker.mouse.click(...AT.settings);
  await seeker.waitForTimeout(600);
  await seeker.screenshot({ path: `${shots}/6-bmseeker-zip.png` });
  const [download] = await Promise.all([seeker.waitForEvent("download", { timeout: 30000 }), seeker.mouse.click(...AT.download)]);
  const zipped = fs.readFileSync(await download.path());
  check(zipped.includes("Seeker Test Song/test.bms") && !zipped.includes(".bmseeker"), `the zip download holds the song (${zipped.length} bytes)`);

  // the GUI at a phone's width
  await seeker.bringToFront();
  await seeker.setViewportSize({ width: 390, height: 844 });
  await seeker.waitForTimeout(1500);
  await seeker.screenshot({ path: `${shots}/5-bmseeker-phone.png` });
} catch (e) {
  check(false, String(e.stack || e));
} finally {
  fs.writeFileSync(`${shots}/console.txt`, `BMSeeker:\n${logs.seeker.join("\n")}\n\nvibeBMS:\n${logs.vibe.join("\n")}\n`);
  await browser.close();
  server.close();
}
process.exit(failed ? 1 : 0);

/** The Link tab: the link, the title, "Into the library". */
async function install(page, link, shot) {
  await page.mouse.click(...AT.link);
  await page.waitForTimeout(400);
  await page.mouse.click(...AT.url);
  await page.keyboard.press("Control+A");
  await page.keyboard.type(link);
  await page.mouse.click(...AT.title);
  await page.keyboard.press("Control+A");
  await page.keyboard.type("Seeker Test Song");
  await page.waitForTimeout(300);
  if (shot) await page.screenshot({ path: shot });
  await page.mouse.click(...AT.into);
}

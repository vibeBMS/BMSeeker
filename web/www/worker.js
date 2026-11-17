// BMSeeker's commands, in a worker (bmseeker-web's wasm): it lists the
// target into a session, runs a command on it (reading the target's files
// synchronously, as only a worker may), then writes what the command
// changed to the target.
//
// In:  {id, op: "open", kind, proxy, dir?}   kind: "vibe" | "folder" | "zip"
//      {id, op: "run", args, file?}           file: one the user downloaded,
//                                             put in the downloads folder for "{file}" in args
//      {id, op: "zip"}
// Out: {id, line, kind}                       a command's line ("record" / "note")
//      {id, note}                             what the worker is doing
//      {id, ok, value} / {id, ok: false, error}
import init, { Session } from "./pkg/bmseeker_web.js";

const ready = init();
let target = null; // {kind, proxy, root (a directory handle or null)}
let session = null;
/** The target's files as listed: path ("/bms/a/x.bms") to File. */
let files = new Map();

// the target's file, read synchronously (the session asks while a command runs)
self.bmseekerRead = (path) => {
  const f = files.get(path);
  if (!f) throw new Error("not listed");
  return new Uint8Array(new FileReaderSync().readAsArrayBuffer(f));
};

// The vibeBMS player's library: songs in bms/ (what it mounts), BMSeeker's
// downloads in bmseeker/. Nothing else of the player's is read or written.
const VIBE_DIRS = ["bms", "bmseeker"];
const LOCK = "vibebms-library";
const CHANNEL = "vibebms-library";

const parts = (path) => path.split("/").filter((p) => p);

async function dirOf(names, create) {
  let d = target.root;
  for (const n of names) d = await d.getDirectoryHandle(n, { create });
  return d;
}

async function list(dir, prefix, out, say) {
  const pending = [];
  for await (const [name, h] of dir.entries()) {
    if (h.kind === "directory") await list(h, prefix + name + "/", out, say);
    else pending.push(h.getFile().then((f) => out.push([prefix + name, f])));
    if (pending.length >= 64) await Promise.all(pending.splice(0));
  }
  await Promise.all(pending);
  if (out.length - say.at >= 2000) {
    say.at = out.length;
    say(`Reading the files: ${out.length}…`);
  }
}

/** A new session on the target, its files listed. */
async function relist(say) {
  await ready;
  session?.free();
  session = new Session(target.kind, target.proxy);
  files = new Map();
  if (!target.root) return;
  const out = [];
  say.at = 0;
  const tops = target.kind === "vibe" ? VIBE_DIRS : [null];
  for (const top of tops) {
    let d = target.root;
    if (top) {
      try {
        d = await d.getDirectoryHandle(top);
      } catch (e) {
        continue;
      }
    }
    await list(d, "/" + (top ? top + "/" : ""), out, say);
  }
  const paths = [], sizes = new Float64Array(out.length), times = new Float64Array(out.length);
  out.forEach(([p, f], i) => {
    files.set(p, f);
    paths.push(p);
    sizes[i] = f.size;
    times[i] = Math.floor(f.lastModified / 1000);
  });
  session.mount(paths, sizes, times);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Write `bytes` to the target's `path`: its File. The player may have the
 * file open (NoModificationAllowedError): asked again a few times. */
async function write(path, bytes) {
  const names = parts(path);
  const name = names.pop();
  const fh = await (await dirOf(names, true)).getFileHandle(name, { create: true });
  for (let i = 0; ; i++) {
    try {
      if (target.kind === "vibe" && fh.createSyncAccessHandle) {
        const h = await fh.createSyncAccessHandle();
        // (older WebKit made truncate / flush / close asynchronous)
        try {
          await h.truncate(0);
          h.write(bytes, { at: 0 });
          await h.flush();
        } finally {
          await h.close();
        }
      } else {
        const w = await fh.createWritable();
        await w.write(bytes);
        await w.close();
      }
      return await fh.getFile();
    } catch (e) {
      if (e.name !== "NoModificationAllowedError" || i >= 5) throw e;
      await sleep(200 << i);
    }
  }
}

async function move(from, to) {
  const src = parts(from), dst = parts(to);
  const name = src.pop(), toName = dst.pop();
  const fh = await (await dirOf(src, false)).getFileHandle(name);
  const toDir = await dirOf(dst, true);
  try {
    await fh.move(toDir, toName);
    return await fh.getFile();
  } catch (e) {
    // no move() here (or it failed): copied, then removed
    const f = await write(to, new Uint8Array(await (await fh.getFile()).arrayBuffer()));
    await remove(from, false);
    return f;
  }
}

async function remove(path, dir) {
  const names = parts(path);
  const name = names.pop();
  try {
    await (await dirOf(names, false)).removeEntry(name, { recursive: dir });
  } catch (e) {
    if (e.name !== "NotFoundError" && e.name !== "TypeMismatchError") throw e;
  }
}

/** What the command changed, written to the target. */
async function flush(say) {
  const changes = session.changes();
  if (changes.length === 0) return;
  const written = [], sizes = [], times = [], failed = [];
  const folders = new Set();
  const top = (p) => parts(p).slice(0, 2).join("/");
  const apply = async () => {
    let n = 0;
    for (const [op, path, a, b] of changes) {
      if (++n % 50 === 0) say(`Saving: ${n} of ${changes.length}…`);
      try {
        let f = null;
        if (op === "remove") {
          await remove(path, a);
          for (const k of [...files.keys()]) if (k === path || k.startsWith(path + "/")) files.delete(k);
        } else if (op === "move") {
          f = await move(path, a);
          files.delete(path);
          folders.add(top(path));
          path = a;
        } else {
          f = await write(path, a);
        }
        folders.add(top(path));
        if (f) {
          files.set(path, f);
          written.push(path);
          sizes.push(f.size);
          times.push(Math.floor(f.lastModified / 1000));
        }
      } catch (e) {
        failed.push(op === "move" ? a : path);
        const why = e.name === "NoModificationAllowedError" ? "vibeBMS has it open, try again" : `${e.name}: ${e.message}`;
        say(`Not saved: ${op === "move" ? a : path} (${why})`);
      }
    }
  };
  if (target.kind === "vibe" && navigator.locks) {
    // never while the player reads its library (it holds the lock shared)
    await navigator.locks.request(LOCK, { mode: "exclusive", ifAvailable: true }, async (lock) => {
      if (lock) return apply();
      say("Waiting for vibeBMS to finish reading its library…");
      await navigator.locks.request(LOCK, { mode: "exclusive" }, apply);
    });
  } else {
    await apply();
  }
  session.settle(written, new Float64Array(sizes), new Float64Array(times), failed);
  if (target.kind === "vibe") {
    // the player's open pages read these folders again
    const changed = [...folders].filter((f) => f.startsWith("bms/"));
    if (changed.length > 0) {
      const ch = new BroadcastChannel(CHANNEL);
      ch.postMessage({ type: "changed", folders: changed });
      ch.close();
    }
  }
}

async function handle(id, m) {
  const say = (note) => postMessage({ id, note });
  switch (m.op) {
    case "open": {
      let root = null;
      if (m.kind === "vibe") root = await navigator.storage.getDirectory();
      else if (m.kind === "folder") root = m.dir;
      target = { kind: m.kind, proxy: m.proxy, root };
      await relist(say);
      return { library: session.library, files: session.count() };
    }
    case "run": {
      // the player's library is listed again: it may have changed it
      if (target.kind === "vibe") await relist(say);
      let args = m.args;
      if (m.file) {
        const at = session.put(m.file.name, new Uint8Array(await m.file.arrayBuffer()));
        args = args.map((a) => (a === "{file}" ? at : a));
      }
      const code = session.run(args, (kind, line) => postMessage({ id, kind, line }));
      // (a zip's files stay here until it is downloaded)
      if (target.root) await flush(say);
      return { code, files: session.count() };
    }
    case "zip":
      return session.zip();
  }
  throw new Error("unknown op " + m.op);
}

let queue = Promise.resolve();
onmessage = (e) => {
  const { id, ...m } = e.data;
  queue = queue
    .then(() => handle(id, m))
    .then(
      (value) => postMessage({ id, ok: true, value }),
      (err) => postMessage({ id, ok: false, error: (err && err.name ? err.name + ": " : "") + ((err && err.message) || err) }),
    );
};

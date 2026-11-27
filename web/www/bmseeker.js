// The page's side of BMSeeker web, for the GUI (gui/cli_js.go): where songs
// go (the target), and the commands run in the worker (worker.js).
//
//   bmseeker.state()                 {kind, label, library, ready, needsPermission, error, ...}
//   bmseeker.onchange = fn           called when the state changes
//   bmseeker.choose(kind)            "vibe" | "folder" | "zip" (a folder: the picker)
//   bmseeker.allow()                 the folder picked before, allowed again
//   bmseeker.run(args, each) -> Promise<exit code>   each("record" | "note", line)
//   bmseeker.pickFile() -> Promise<name | "">   a file for the next run's "{file}"
//   bmseeker.downloadZip() -> Promise<number of bytes>
//
// ?proxy=<url> uses another CORS proxy, on a page served from this computer
// only (tests: a local one): anywhere else, a link to the page would choose
// the server its every request goes through.
(() => {
  const local = location.hostname === "localhost" || location.hostname === "127.0.0.1";
  const asked = local && new URLSearchParams(location.search).get("proxy");
  const PROXY = asked || "https://proxy.vibebms.workers.dev/";
  const LABELS = {
    vibe: "vibeBMS web library (this browser)",
    folder: "A folder on this computer (Chrome/Edge)",
    zip: "Download as zip",
  };
  const worker = new Worker(new URL("worker.js", document.currentScript.src), { type: "module" });
  let next = 1;
  const waiting = new Map();
  worker.onmessage = (e) => {
    const m = e.data;
    const w = waiting.get(m.id);
    if (!w) return;
    if (m.line !== undefined) w.line(m.kind, m.line);
    else if (m.note !== undefined) w.note(m.note);
    else {
      waiting.delete(m.id);
      m.ok ? w.resolve(m.value) : w.reject(new Error(m.error));
    }
  };
  const ask = (msg, line = () => {}, note = () => {}) =>
    new Promise((resolve, reject) => {
      const id = next++;
      waiting.set(id, { resolve, reject, line, note });
      worker.postMessage({ id, ...msg });
    });

  const supports = {
    vibe: !!(navigator.storage && navigator.storage.getDirectory),
    folder: typeof window.showDirectoryPicker === "function",
    zip: true,
  };
  let st = { kind: "", label: "", library: "", ready: false, needsPermission: false, folder: "", files: 0, error: "", supports };
  let picked = null;

  const set = (patch) => {
    st = { ...st, ...patch };
    try {
      bmseeker.onchange && bmseeker.onchange();
    } catch (e) {
      console.error(e);
    }
  };

  // the folder picked, kept for the next visit (asked to be allowed again)
  const db = () =>
    new Promise((resolve, reject) => {
      const r = indexedDB.open("bmseeker", 1);
      r.onupgradeneeded = () => r.result.createObjectStore("handles");
      r.onsuccess = () => resolve(r.result);
      r.onerror = () => reject(r.error);
    });
  const idb = async (mode, f) => {
    const d = await db();
    return new Promise((resolve, reject) => {
      const t = d.transaction("handles", mode);
      const r = f(t.objectStore("handles"));
      t.oncomplete = () => resolve(r.result);
      t.onerror = () => reject(t.error);
    });
  };
  const saveHandle = (h) => idb("readwrite", (s) => s.put(h, "folder"));
  const loadHandle = () => idb("readonly", (s) => s.get("folder")).catch(() => null);

  const remember = (kind) => {
    try {
      localStorage.setItem("bmseeker.target", kind);
    } catch (e) {}
  };

  async function open(kind, dir) {
    set({ kind, label: LABELS[kind], ready: false, needsPermission: false, error: "", folder: dir ? dir.name : "" });
    try {
      const got = await ask({ op: "open", kind, proxy: PROXY, dir }, undefined, (n) => set({ error: n }));
      set({ library: got.library, files: got.files, ready: true, error: "" });
      remember(kind);
    } catch (e) {
      set({ error: String(e.message || e) });
    }
  }

  const bmseeker = {
    onchange: null,
    vibeUrl: location.origin + "/vibe/",
    state: () => st,
    async choose(kind) {
      if (!supports[kind]) {
        set({ error: `${LABELS[kind]}: this browser cannot` });
        return;
      }
      if (kind === "folder") {
        let dir;
        try {
          dir = await window.showDirectoryPicker({ id: "bmseeker", mode: "readwrite" });
        } catch (e) {
          if (e.name !== "AbortError") set({ error: String(e.message || e) });
          return;
        }
        await saveHandle(dir).catch(() => {});
        return open("folder", dir);
      }
      return open(kind);
    },
    async allow() {
      const dir = await loadHandle();
      if (!dir) return bmseeker.choose("folder");
      if ((await dir.requestPermission({ mode: "readwrite" })) === "granted") return open("folder", dir);
      set({ error: `${dir.name}: not allowed` });
    },
    run(args, each) {
      const file = args.includes("{file}") ? picked : undefined;
      if (file) picked = null;
      return ask({ op: "run", args, proxy: PROXY, file }, each, (n) => each("note", n)).then((r) => {
        set({ files: r.files });
        return r.code;
      });
    },
    pickFile() {
      return new Promise((resolve) => {
        const input = document.createElement("input");
        input.type = "file";
        input.accept = ".zip,.rar,.7z,.lzh,.lha,.tar,.tgz,.tar.gz,.gz,.bms,.bme,.bml,.pms,.bmson";
        input.onchange = () => {
          picked = input.files[0] || null;
          resolve(picked ? picked.name : "");
        };
        input.oncancel = () => resolve("");
        input.click();
      });
    },
    async downloadZip() {
      const bytes = await ask({ op: "zip" });
      const a = document.createElement("a");
      a.href = URL.createObjectURL(new Blob([bytes], { type: "application/zip" }));
      a.download = "BMSeeker songs.zip";
      document.body.append(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(a.href), 60000);
      return bytes.length;
    },
  };
  globalThis.bmseeker = bmseeker;

  // where songs went last time; the player's library at first
  (async () => {
    let kind = "";
    try {
      kind = localStorage.getItem("bmseeker.target") || "";
    } catch (e) {}
    if (kind === "folder" && supports.folder) {
      const dir = await loadHandle();
      if (dir && (await dir.queryPermission({ mode: "readwrite" })) === "granted") return open("folder", dir);
      if (dir) return set({ kind: "folder", label: LABELS.folder, folder: dir.name, needsPermission: true });
    }
    if (!supports[kind] || kind === "folder") kind = supports.vibe ? "vibe" : "zip";
    open(kind);
  })();
})();

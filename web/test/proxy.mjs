// The page's script (web/www/bmseeker.js) takes another CORS proxy from
// ?proxy= only on a page served from this computer (as e2e.mjs serves it);
// anywhere else the page keeps to its own.
//
//   node web/test/proxy.mjs
import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";

const script = fs.readFileSync(new URL("../www/bmseeker.js", import.meta.url), "utf8");
const OWN = "https://proxy.vibebms.workers.dev/";

// The proxy the page at `url` opens its worker with.
function proxyAt(url) {
  const posted = [];
  class Worker {
    postMessage(m) {
      posted.push(m);
    }
  }
  const page = {
    location: new URL(url),
    document: { currentScript: { src: new URL("bmseeker.js", url).href } },
    navigator: { storage: { getDirectory() {} } },
    localStorage: { getItem: () => null, setItem() {} },
    URL,
    URLSearchParams,
    Worker,
    console,
  };
  page.window = page;
  vm.runInNewContext(script, page);
  return posted.find((m) => m.op === "open").proxy;
}

const other = "https://other.example/";
const asking = `?proxy=${encodeURIComponent(other)}`;
for (const at of ["https://vibebms.github.io/BMSeeker/", "https://localhost.other.example/", "https://127.0.0.1.other.example/", "http://[::1]:8080/"]) {
  assert.equal(proxyAt(at + asking), OWN, at);
}
for (const at of ["http://localhost:8080/BMSeeker/", "http://127.0.0.1:8080/", "https://localhost/"]) {
  assert.equal(proxyAt(at + asking), other, at);
  assert.equal(proxyAt(at), OWN, at);
}
console.log("ok");

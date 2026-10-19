// Before/after file-system inventory (Phase 3D-3). READ-ONLY: it lists a directory tree without following links, hashes ordinary files with
// SHA-256 and records sizes; it opens nothing for writing, starts nothing and uses no network. It never executes or interprets a file.
//
//   node tools/fs_inventory.mjs snapshot <directory> <output.json>   write an inventory
//   node tools/fs_inventory.mjs diff <before.json> <after.json>      print added / removed / changed entries (exit 0 always; the reader decides)
//
// Used by tools/gui_install_flow.mjs and by the procedure of the first real pilot (docs/phase-3d3/PILOT-PROCEDURE.md).
import { createHash } from "node:crypto";
import { lstatSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, sep } from "node:path";

const HASH_LIMIT = 64 * 1024 * 1024;

export function inventory(root) {
  const out = {};
  const walk = (dir) => {
    let names = [];
    try { names = readdirSync(dir).sort(); } catch { return; }
    for (const n of names) {
      const p = join(dir, n);
      const rel = p.slice(root.length + 1).split(sep).join("/");
      let st;
      try { st = lstatSync(p); } catch { out[rel] = { kind: "unreadable" }; continue; }
      if (st.isSymbolicLink()) { out[rel] = { kind: "link" }; continue; }
      if (st.isDirectory()) { out[rel + "/"] = { kind: "dir" }; walk(p); continue; }
      if (!st.isFile()) { out[rel] = { kind: "other" }; continue; }
      const e = { kind: "file", size: st.size, links: st.nlink };
      if (st.size <= HASH_LIMIT) {
        try { e.sha256 = createHash("sha256").update(readFileSync(p)).digest("hex"); } catch { e.sha256 = null; }
      }
      out[rel] = e;
    }
  };
  walk(root);
  return out;
}

export function diff(a, b) {
  const added = Object.keys(b).filter((k) => !(k in a));
  const removed = Object.keys(a).filter((k) => !(k in b));
  const changed = Object.keys(a).filter((k) => k in b && JSON.stringify(a[k]) !== JSON.stringify(b[k]));
  return { added, removed, changed };
}

if (import.meta.url === `file:///${process.argv[1].replaceAll("\\", "/")}`) {
  const [cmd, x, y] = process.argv.slice(2);
  if (cmd === "snapshot") {
    writeFileSync(y, JSON.stringify(inventory(x), null, 1));
  } else if (cmd === "diff") {
    console.log(JSON.stringify(diff(JSON.parse(readFileSync(x, "utf8")), JSON.parse(readFileSync(y, "utf8"))), null, 1));
  } else {
    console.error("usage: snapshot <dir> <out.json> | diff <before.json> <after.json>");
    process.exit(2);
  }
}

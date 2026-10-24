// Vérifie que l'application compilée respecte la CSP du serveur :
// aucun script ni style en ligne, aucune ressource chargée depuis une autre origine.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const dist = new URL("../dist/", import.meta.url).pathname.replace(/^\/(\w:)/, "$1");
const problems = [];

const html = readFileSync(join(dist, "index.html"), "utf8");
for (const m of html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)) {
  if (!/\bsrc=/.test(m[1]) || m[2].trim()) problems.push("script en ligne dans index.html");
}
if (/<style\b/.test(html) || /\sstyle=/.test(html)) problems.push("style en ligne dans index.html");
if (/\son[a-z]+=/.test(html)) problems.push("gestionnaire d'événement en ligne dans index.html");
for (const m of html.matchAll(/\b(?:src|href)="([^"]+)"/g)) {
  if (/^(?:[a-z]+:)?\/\//i.test(m[1])) problems.push(`ressource externe : ${m[1]}`);
}

function walk(dir) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) walk(p);
    else if (p.endsWith(".map")) problems.push(`carte de source publiée : ${e.name}`);
  }
}
walk(dist);

if (problems.length) {
  console.error("Contrôle de la CSP : ÉCHEC\n- " + problems.join("\n- "));
  process.exit(1);
}
console.log("Contrôle de la CSP : OK");

// Deliberately minimal: esbuild bundles two entry points to plain IIFEs,
// then the manifest/icons/popup HTML are copied as-is. No framework, no
// bundler config beyond what a two-script MV3 extension actually needs.
import { build, context } from "esbuild";
import { cpSync, mkdirSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = dirname(fileURLToPath(import.meta.url));
const outdir = join(root, "dist");
const watch = process.argv.includes("--watch");

rmSync(outdir, { recursive: true, force: true });
mkdirSync(outdir, { recursive: true });

const options = {
  entryPoints: [join(root, "src/background/service-worker.ts"), join(root, "src/popup/popup.ts")],
  outdir,
  outbase: join(root, "src"),
  bundle: true,
  format: "iife",
  target: "chrome116",
  sourcemap: true,
  logLevel: "info",
};

function copyStatic() {
  cpSync(join(root, "manifest.json"), join(outdir, "manifest.json"));
  cpSync(join(root, "public/icons"), join(outdir, "icons"), { recursive: true });
  cpSync(join(root, "public/fonts"), join(outdir, "fonts"), { recursive: true });
  mkdirSync(join(outdir, "popup"), { recursive: true });
  cpSync(join(root, "src/popup/popup.html"), join(outdir, "popup/popup.html"));
  cpSync(join(root, "src/popup/popup.css"), join(outdir, "popup/popup.css"));
}

if (watch) {
  const ctx = await context(options);
  copyStatic();
  await ctx.watch();
  console.log("Watching browser-extension sources... (Ctrl+C to stop)");
} else {
  await build(options);
  copyStatic();
  console.log(`Built extension to ${outdir}`);
}

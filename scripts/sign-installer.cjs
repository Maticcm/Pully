const { spawnSync } = require("node:child_process");
const path = require("node:path");

const version = require("../package.json").version;
const keyPath = process.env.TAURI_SIGNING_PRIVATE_KEY_PATH;
if (!keyPath) {
  console.error("Set TAURI_SIGNING_PRIVATE_KEY_PATH to your updater private key file.");
  process.exit(1);
}
const installer = path.resolve(__dirname, `../src-tauri/target/release/bundle/nsis/Pully_${version}_x64-setup.exe`);
const cli = path.resolve(__dirname, "../node_modules/@tauri-apps/cli/tauri.js");
const result = spawnSync(process.execPath, [cli, "signer", "sign", "--private-key-path", keyPath, "--password", process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "", installer], { stdio: "inherit" });
process.exit(result.status ?? 1);

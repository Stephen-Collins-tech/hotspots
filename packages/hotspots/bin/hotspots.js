#!/usr/bin/env node
"use strict";

const { execFileSync } = require("child_process");
const path = require("path");
const os = require("os");

function getBinaryPath() {
  const platform = os.platform();
  const arch = os.arch();

  let pkgName;
  let binName;

  if (platform === "linux" && arch === "x64") {
    pkgName = "@stephencollinstech/hotspots-linux-x64";
    binName = "hotspots";
  } else if (platform === "darwin" && arch === "arm64") {
    pkgName = "@stephencollinstech/hotspots-darwin-arm64";
    binName = "hotspots";
  } else if (platform === "win32" && arch === "x64") {
    pkgName = "@stephencollinstech/hotspots-win32-x64";
    binName = "hotspots.exe";
  } else {
    console.error(
      `hotspots: unsupported platform ${platform}/${arch}\n` +
        `Supported: linux/x64, darwin/arm64, win32/x64\n` +
        `Install from source: cargo install hotspots-cli`
    );
    process.exit(1);
  }

  try {
    return require.resolve(`${pkgName}/bin/${binName}`);
  } catch {
    console.error(
      `hotspots: could not find binary package ${pkgName}\n` +
        `Try reinstalling: npm install hotspots`
    );
    process.exit(1);
  }
}

const bin = getBinaryPath();

try {
  execFileSync(bin, process.argv.slice(2), { stdio: "inherit" });
} catch (err) {
  process.exit(err.status ?? 1);
}

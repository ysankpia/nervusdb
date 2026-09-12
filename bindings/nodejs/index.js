// Native binding loader.
//
// The package ships one `.node` per supported platform, named
// `nervusdb.<triple>.node`, and this file picks the one matching the running
// process. Three things this replaces, all of which were wrong before:
//
//   1. It searched `target/{release,debug}/libnervusdb_node.{dylib,so,dll}`.
//      `require` cannot load a `.dylib` or `.so` at all — it fails with
//      "Invalid or unexpected token", which points nowhere near the cause. Those
//      paths only ever worked because a hand-copied `nervusdb.node` existed on
//      the developer's machine, and that file is gitignored so CI had none.
//      That is how the first CI run of the SDK job failed.
//   2. `napi build --platform` emits `nervusdb.<triple>.node`, but the loader only
//      looked for `nervusdb.node` — so the published package could not have
//      loaded its own binary.
//   3. It could only ever find the build machine's platform, so a published
//      tarball was useless to anyone else.
//
// Kept explicit rather than generated: the mapping is four lines, and a reader
// can verify it against `process.platform` without trusting a code generator.

const { existsSync } = require("fs");
const { join } = require("path");

// Rust target triples this package is built for (see `napi.triples` in
// package.json). Order does not matter; the lookup is exact.
const TRIPLES = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "aarch64-unknown-linux-gnu",
  "x86_64-unknown-linux-gnu",
];

/** Map the running process to a triple, or null if we do not build for it. */
function currentTriple() {
  const { platform, arch } = process;
  if (platform === "darwin") {
    if (arch === "arm64") return "aarch64-apple-darwin";
    if (arch === "x64") return "x86_64-apple-darwin";
  }
  if (platform === "linux") {
    // `arch` is "arm64" or "x64". musl has no separate build; on Alpine the glibc
    // binary will fail to load and the error below names the platform, which is
    // the honest outcome rather than a silent wrong-arch attempt.
    if (arch === "arm64") return "aarch64-unknown-linux-gnu";
    if (arch === "x64") return "x86_64-unknown-linux-gnu";
  }
  return null;
}

function load() {
  const triple = currentTriple();
  const candidates = [];

  if (triple) {
    candidates.push(join(__dirname, `nervusdb.${triple}.node`));
  }
  // Un-suffixed name: what `napi build` produces without `--platform`, and what a
  // local `npm run build:debug` leaves behind. Last so a platform-specific build
  // always wins when both exist.
  candidates.push(join(__dirname, "nervusdb.node"));

  for (const candidate of candidates) {
    if (!existsSync(candidate)) continue;
    try {
      return require(candidate);
    } catch (err) {
      throw new Error(
        `NervusDB found a native binding at ${candidate} but could not load it: ` +
          `${err.message}\n` +
          `This usually means the file was built for a different platform or ` +
          `architecture.`,
      );
    }
  }

  const supported = TRIPLES.join(", ");
  const running = triple
    ? `${process.platform}-${process.arch} (${triple})`
    : `${process.platform}-${process.arch} (not a supported target)`;
  throw new Error(
    `NervusDB has no native binding for this platform.\n` +
      `  running:   ${running}\n` +
      `  available: ${supported}\n` +
      (triple
        ? `The package is present but the binary for ${triple} is missing; ` +
          `this is a packaging bug, not a configuration problem.`
        : `Build it from source (see bindings/nodejs/README.md) or open an issue.`),
  );
}

const nativeBinding = load();

module.exports = nativeBinding;
module.exports.NervusDb = nativeBinding.NervusDb;
// Transaction alias: napi registers it under the Rust type name.
module.exports.Transaction = nativeBinding.Transaction;

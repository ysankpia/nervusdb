const { existsSync } = require("fs");
const { join } = require("path");

let nativeBinding = null;

const possiblePaths = [
  join(__dirname, "graphlite.node"),
  join(__dirname, "..", "..", "target", "release", "libgraphlite_node.dylib"),
  join(__dirname, "..", "..", "target", "debug", "libgraphlite_node.dylib"),
  join(__dirname, "..", "..", "target", "release", "libgraphlite_node.so"),
  join(__dirname, "..", "..", "target", "debug", "libgraphlite_node.so"),
  join(__dirname, "..", "..", "target", "release", "graphlite_node.dll"),
  join(__dirname, "..", "..", "target", "debug", "graphlite_node.dll"),
];

for (const p of possiblePaths) {
  if (existsSync(p)) {
    try {
      nativeBinding = require(p);
      break;
    } catch (e) {
      // continue searching
    }
  }
}

if (!nativeBinding) {
  try {
    nativeBinding = require("./graphlite.node");
  } catch (err) {
    throw new Error(
      `Failed to load native binding for GraphLite: ${err.message}`,
    );
  }
}

module.exports = nativeBinding;
module.exports.GraphLite = nativeBinding.GraphLite;

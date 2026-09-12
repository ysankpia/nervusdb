const { existsSync } = require("fs");
const { join } = require("path");

let nativeBinding = null;

const possiblePaths = [
  join(__dirname, "nervusdb.node"),
  join(__dirname, "..", "..", "target", "release", "libnervusdb_node.dylib"),
  join(__dirname, "..", "..", "target", "debug", "libnervusdb_node.dylib"),
  join(__dirname, "..", "..", "target", "release", "libnervusdb_node.so"),
  join(__dirname, "..", "..", "target", "debug", "libnervusdb_node.so"),
  join(__dirname, "..", "..", "target", "release", "nervusdb_node.dll"),
  join(__dirname, "..", "..", "target", "debug", "nervusdb_node.dll"),
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
    nativeBinding = require("./nervusdb.node");
  } catch (err) {
    throw new Error(
      `Failed to load native binding for NervusDb: ${err.message}`,
    );
  }
}

module.exports = nativeBinding;
module.exports.NervusDb = nativeBinding.NervusDb;
// 事务句柄别名（napi 依据 Rust 类型名注册为 `Transaction`）
module.exports.Transaction = nativeBinding.Transaction;

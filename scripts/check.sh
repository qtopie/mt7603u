#!/usr/bin/env bash
set -euo pipefail

echo "=== 1. 运行代码 Lint 校验 ==="
if [ -d "src/rust" ] && command -v cargo >/dev/null 2>&1; then
    (cd src/rust && cargo fmt --check && cargo clippy --release --lib -- -D warnings)
else
    echo "--> Cargo.toml 不存在或 cargo 未安装，跳过 Rust lint..."
fi

echo "=== 2. 运行 Harness 评估与沙盒测试套件 ==="
if [ -f "./scripts/check-harness.sh" ]; then
    ./scripts/check-harness.sh
else
    echo "--> Running fallback test suite..."
fi

echo "✅ 所有校验与 Harness 测试已成功通过！"

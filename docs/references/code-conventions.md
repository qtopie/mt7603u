# Code Conventions (Rust)

## 1. Formatting & Naming
- 遵循 Rust 官方命名规范（snake_case 函数/变量、CamelCase 类型、SCREAMING_SNAKE_CASE 常量）。
- 格式化统一使用 `rustfmt`，提交前必须通过 `cargo fmt --check`。
- 安全红线：`unsafe` 必须包裹在最小化 API 中并附 `// SAFETY:` 注释说明不变量。

## 2. Error Handling
- 使用 `Result<T, E>` 传播错误，为错误类型实现 `core::error::Error` / `Display`。
- 所有错误必须明确捕获与处理，禁止吞掉异常（生产路径禁止裸 `unwrap()` / `expect()`）。
- 错误信息需具备上下文说明（使用 `context()` 或自定义错误枚举）。

## 3. Logging & Telemetry
- 使用内核日志宏（`pr_info` / `dev_dbg` 等）或结构化日志 crate。
- 禁止输出密码、密钥、Token 等敏感数据。

## 4. Security Red Lines
- 正确处理 DMA / MMIO 边界与缓冲区越界（`Vec` / `slice` 边界检查）。
- 禁止将用户态指针直接当作内核指针使用；禁止未校验长度的内存拷贝。

# Testing Guidelines & Standards

为了保证代码质量与自动化测试的高效运行，Agent 和开发者必须遵循以下测试规范：

## 1. 测试层级划分与目录约定
- **单元测试 (Unit Tests):** 放在与被测代码同级的目录下。
  - *原则:* 运行速度极快，**严禁产生真实网络 I/O 或本地文件硬盘写盘**。
- **集成/E2E 测试 (Integration Tests):** 统一放在根目录 `testings/` 下。
  - *原则:* 允许通过 Mock Server 或内存数据库/桩模拟真实依赖流转。

## 2. 框架与工具链
- **Test Runner:** Rust 标准 `#[cfg(test)] mod tests` + `cargo test`；内核相关模块可借助 `kernel-hal` / 用户态模拟层运行。
- **Mock 工具:** 硬件访问（USB 端点、寄存器读写）必须抽象为 `Trait`（如 `RegOps` / `UsbBus`），测试中使用 `mockall` 或手写假实现（`harness/mocks/`）模拟。
- **测试命名规范:** `test_<component>_<scenario>` (Rust snake_case)
  - 示例: `test_phy_reg_init_sequence`, `test_ring_full_overwrite`

## 3. Mock 编写原则
- 针对外部依赖（数据库、缓存、RPC 远程服务、第三方 API、硬件 Gateway / USB 设备 / 寄存器），**必须抽象为 Interface/Trait 并使用 Mock**。
- 不得在测试代码中硬编码任何本地绝对路径或敏感 Token。
- 禁止写敷衍测试（例如仅测试 Getter/Setter），测试必须覆盖真实边界分支与异常流程。

## 4. 跑测试的标准命令
- **运行全量单元测试:** `cargo test --all-features`
- **运行格式化/静态检查:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
- **运行指定规范的集成测试:** 按照 `specs/modules/*.spec.md` 中的 `Mapped Test` 指定的入口运行测试。

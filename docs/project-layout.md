# Project Layout & Module Boundaries

## Directory Structure
- `src/`: Rust 驱动源码（`no_std` + `kernel`/`alloc` feature）
- `specs/`: Single Source of Truth (SSOT) 规范与契约
  - `specs/modules/`: 各模块行为契约与 BDD 场景（如 `regops.spec.md`, `mac.spec.md`）
  - `specs/apis/`: 对外 API / ioctl / net_device 接口契约
  - `specs/schemas/`: 寄存器位域、描述符结构、固件数据包格式
- `harness/`: Harness 评估与测试夹具工程
  - `harness/fixtures/`: 寄存器 dump、固件帧样例、关联状态机事件序列
  - `harness/mocks/`: USB 总线与寄存器访问 Mock（`RegOps` trait 假实现）
  - `harness/runners/`: BDD 场景套件执行器与不变量断言点
  - `harness/harness.env`: Local Shell 沙盒环境配置
- `docs/`: 方案设计、RFCs、Bug RCA、规范文档
  - `docs/system-design.md`: 系统总体架构设计
  - `docs/rfcs/`: 技术提案（模板 `template.md`）
  - `docs/bugs/`: Bug RCA（模板 `template.md`）
  - `docs/testing/guidelines.md`: 全局测试规范与工具链指引
  - `docs/testing/harness-engineering.md`: Harness 评估与测试套件指南
- `testings/`: 自动化集成测试与 E2E 测试集（用户态可编译模拟层）
- `scripts/`: 构建与校验自动化工具链（`check.sh`, `check-harness.sh`, `check-spec-drift.sh`）
- `.agents/`: Agent 动态看板（`TASK.md`）、Agent 规则与 MCP 配置

## Module Dependencies Rule
- 模块间依赖必须保持单向流转，禁止循环依赖。
- 业务代码实现必须时刻保持与 `specs/` 契约一致。
- 单元测试放在与被测代码同级的目录下；集成/E2E 测试统一放在 `testings/` 下；测试夹具与桩集中在 `harness/` 中。

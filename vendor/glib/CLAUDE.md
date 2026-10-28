# glib/
> L2 | 父级: ../CLAUDE.md
crates.io glib 0.18.5（MIT）的完整发布快照；保留上游注释与文件结构，避免把第三方代码改写成应用业务层。
唯一行为差异来自官方 gtk-rs-core b5a4071e439bef2b5eea76c3aa25e5ae84839e34；scripts/test-glib-backport.py 校验全部来源字节并运行优化迭代器测试。
成员清单
.cargo_vcs_info.json: 上游发布元数据/构建样本，随原包保留并受来源摘要校验。
COPYRIGHT: 上游发布元数据/构建样本，随原包保留并受来源摘要校验。
Cargo.toml: 上游发布规范化依赖与 feature 契约，保留 0.18 ABI 与版本；宿主通过 Cargo patch 消费。
Cargo.toml.orig: 上游原始工作区清单，仅用于来源对照，不参与本项目依赖解析。
Gir.toml: 上游发布元数据/构建样本，随原包保留并受来源摘要校验。
Gir_GObject.toml: 上游发布元数据/构建样本，随原包保留并受来源摘要校验。
LICENSE: 上游 MIT 许可，完整保留原版权与授权边界。
README.md: 上游发布元数据/构建样本，随原包保留并受来源摘要校验。
UPSTREAM.json: 源包 SHA-256、官方修复提交与全部原文件摘要；独立回归验证只有两行功能差异。
benches/: 上游字符串基准，不参与宿主生产构建。
src/: GLib 0.18 Rust 绑定；除 variant_iter.rs 的上游出参修复外保持逐字快照。
tests/: 上游集成与编译回归样本；生产宿主不导出这些测试能力。
[PROTOCOL]: 变更时更新此头部，然后检查 CLAUDE.md

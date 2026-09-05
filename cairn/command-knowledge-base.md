---
type: project_topic
status: active
summary: 命令知识库设计的可复用模式：事实/策略分离、条目开放谓词封闭、槽位跟着消费机制走、等价类与属性的运行时边界。格式与机制细节以 doc/design.md 为单一事实源，决策论证见 doc/decisions.md。
tags: [crush_tether, architecture, knowledge-base, design-pattern]
contains: [experience, lesson]
created: 2026-09-06
updated: 2026-09-06
related: [doc/design.md, doc/decisions.md, cairn/zero-builtin-policy-seeding.md]
authoring_mode: ai_generated
---
# 命令知识库设计模式

## 形成背景

2026-09-06 设计评审暴露两类「命令约定关联」问题：等价方言（`npm exec`/`npm x` ≡ `npx`，一处 confirm 一处 allow 即构成绕过）与双桶冲突（`git reset` 同落 confirm/deny 两桶）。初版方案把 alias 表放进用户 rules.toml，用户纠正：命令关联是通用事实，不应由用户配置承载——由此确立独立知识库（bucket 框架）。格式与机制细节以 `doc/design.md`「命令知识库（bucket 框架，定稿）」为单一事实源，决策论证与被否决方案见 `doc/decisions.md` D-01/D-06，本笔记只沉淀跨设计本体的可复用模式与教训。

## 可复用模式

- **事实与策略分离**：权限/门禁类系统里，「命令会写文件」是事实、「命令该被确认」是策略；事实数据随软件分发、社区可维护，策略只属于用户。知识库再大也推不出一条裁决——这是与「零内置策略」并存而不冲突的边界辨析（见 [zero-builtin-policy-seeding.md](zero-builtin-policy-seeding.md)）。
- **条目开放、谓词封闭**：数据条目人人可写，但联系类型（槽位）由引擎版本定义、一类槽位绑定一个消费机制；不提供任意谓词。开放数据获得生态，封闭谓词防止知识库演化为绕过策略层的隐性规则系统。
- **槽位跟着消费机制走**：一个维度只有存在明确消费者（归一 / lint / 脚本数据源）才配拥有槽位；没有消费者的属性是死数据。该标准可裁定（每个新槽位都能回答「谁消费它」），防元数据无限膨胀。
- **等价类参与运行时、属性只进检查**：别名/flag 等价（归一）改变查表输入，必须在运行时；读写属性只影响 lint 建议与脚本数据源，不进引擎判定路径——否则删掉知识库裁决就变，与「可删」自相矛盾。

## 教训

- **等价命令是权限门的经典绕过面**：`npx` 被挡而 `npm exec` 放行 = 同一件事两种裁决；枚举式配置天然漏等价形态，必须靠归一机制（查表前改写规范形）兜住，且日志要记录归一链保持可追溯。
- **评审对照基准要先钉死**：本轮评审最初以「与 guard.py 判定表一致」为偏差标准，被用户纠正——参考对象不是验收标准，否则有意的设计改进会被误判为回归（`doc/decisions.md` D-05）。

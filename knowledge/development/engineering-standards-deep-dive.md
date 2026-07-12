---
id: engineering-standards-deep-dive
title: 工程规范深度标准（商业级·反屎山·通用方法论）
domain: development
category: 01-standards
difficulty: advanced
tags: [工程规范, engineering-standards, 分层, 分包, 命名规范, 反屎山, 代码质量, 单一职责, layering, packaging, naming]
quality_score: 93
last_updated: 2026-07-12
---

# 工程规范深度标准（商业级·反屎山·通用方法论）

> 语言无关的工程纪律：一套跨所有技术栈通用、专门用来防止"屎山"的方法论。商业级代码不是"能跑就行"，而是**命名有领域语义、依赖单向可控、业务规则有明确归属、单元有硬性体量上限**。写代码前先定结构，让文件名与目录本身就是架构。栈相关的细则（前端分层、后端服务层、具体框架惯例）在专门文件里；本文只讲所有栈共享的骨架规则。
>
> 本文示例用中性伪代码；个别地方并列 2-3 种语言以点明差异。所有规则都是可执行的技术判据，不是风格偏好。

## 0. 一句话原则

**命名承载领域语义，依赖只朝一个方向流，业务规则集中在领域/服务层，任何单元（文件/函数/类）都有硬性体量上限——超限即拆分，不是靠自觉，是靠可度量的红线。**

---

## 1. 命名与字段规范

命名是最廉价也最高杠杆的可读性投资：好名字让代码自解释，坏缩写让每次阅读都要"翻译"。

### 1.1 命名法按语言族选择，然后全项目统一

- 类型/类名：跨语言几乎都用 `PascalCase`（`OrderService`、`UserProfile`）。
- 变量/函数：随语言族——大括号系（多数 C 后裔）用 `camelCase`；缩进系与部分系统语言用 `snake_case`。**选定后全项目一致**，禁止一个文件两种风格混用。
- 常量：`UPPER_SNAKE_CASE`（`MAX_RETRY_COUNT`）。
- 私有/内部成员：按语言约定加前缀或用可见性修饰符，别用命名 hack 冒充封装。

### 1.2 领域语义命名，禁止无业务含义缩写

名字要说清"这是业务里的什么"，而不是省几个字母。禁用缩写清单（示例，非穷举）：

| 禁用 | 用 | 禁用 | 用 |
|---|---|---|---|
| `usr` / `u` | `user` | `amt` | `amount` |
| `cnt` | `count` | `qty` | `quantity` |
| `tmp` / `temp` | 说清它是什么（`draftOrder`） | `flg` | 具体布尔名（`isPaid`） |
| `val` / `val2` | `price` / `discountedPrice` | `data` / `data2` | `orderList` / `orderSummary` |
| `res` / `resp` | `orderResponse` | `obj` / `o` | 具体实体名 |
| `mgr` | `manager` | `svc` | `service` |
| `proc` | `process` / `processor` | `idx` | `index`（循环计数器 `i` 可留） |

例外白名单：领域内公认且无歧义的缩写可保留（`id`、`url`、`http`、`sku`、`vip`、`utc`）。判据：**这个缩写会不会让新同事需要问"这是什么"**——会，就展开。

```text
// BAD：无语义缩写，读者必须逆向猜测
fn calc(u, amt, flg) {
    let tmp = amt * 0.9
    if flg { return tmp } else { return amt }
}

// GOOD：名字自解释业务意图
fn final_price(user: User, list_price: Money, is_member: bool) -> Money {
    let member_price = list_price * 0.9
    if is_member { member_price } else { list_price }
}
```

### 1.3 字段命名约定（按类型固定套路）

- **布尔**：加 `is` / `has` / `can` / `should` 前缀，表达一个判定，不用否定式。
  - GOOD：`isActive`、`hasPermission`、`canRetry`、`shouldNotify`
  - BAD：`active`（易与状态枚举混）、`disabled`（否定式导致 `!disabled` 双重否定）、`flag`
- **时间**：统一后缀，并在类型/文档里锁定时区与单位。
  - 时间点用 `xxxAt`（`createdAt`、`expiredAt`），存储/传输一律 **UTC**，展示层再转本地。
  - 时长用带单位后缀：`timeoutMs`、`ttlSeconds`、`retryDelayMs`——**单位进名字**，杜绝"这个 300 是毫秒还是秒"。
  - BAD：`time`、`date`（无时区无单位）、`timeout`（单位不明）。
- **金额**：单位与精度显式化，禁止用浮点存钱。
  - 命名带单位：`priceCents`（整数分）或 `amount: Money`（值对象，内含币种+精度）。
  - BAD：`price: float = 19.99`（浮点累加误差）、`total`（币种未知）。
  - GOOD：`amountCents: i64` 或 `Money { cents: i64, currency: "CNY" }`。
- **枚举**：类型名单数、成员领域化、不用魔法数。
  - GOOD：`enum OrderStatus { Pending, Paid, Shipped, Refunded }`
  - BAD：`status: int // 0=待付 1=已付`（魔法数散落各处）。
- **集合**：复数名表达"多个"。
  - GOOD：`orders: List<Order>`、`userIdsToNotify`
  - BAD：`orderList`（`List` 后缀冗余，`orders` 已够）、`data`。
- **常量**：语义化，禁止裸魔法数/字符串出现在逻辑里。
  - GOOD：`const MAX_UPLOAD_BYTES = 10 * 1024 * 1024`
  - BAD：代码里到处出现 `10485760`。

### 1.4 DTO / VO / Entity / DO 的区分与命名后缀

不同层的数据对象职责不同，用后缀锁定层次，禁止一个对象从数据库一路裸传到前端。

| 类型 | 职责 | 归属层 | 命名后缀 |
|---|---|---|---|
| **DTO**（Data Transfer Object） | 跨边界传输（接口出入参） | 接入/应用层 | `OrderDTO` / `CreateOrderRequest` / `OrderResponse` |
| **VO**（View Object / Value Object） | 面向展示的视图模型 / 不可变值对象 | 应用/领域层 | `OrderSummaryVO` / `Money`（值对象） |
| **Entity / DO**（Domain Object / Data Object） | 领域实体 / 持久化映射 | 领域/基础设施层 | `Order`（实体） / `OrderPO`（持久化对象） |

```text
// BAD：数据库实体直接当接口返回，字段（含敏感/内部字段）全裸奔到前端
GET /orders/1  ->  返回 OrderEntity { id, userId, internalCost, dbShardKey, ... }

// GOOD：实体在领域层，接口返回裁剪过的 DTO，字段可控、敏感字段不出边界
GET /orders/1  ->  OrderResponse { id, status, totalCents, items }
// 由 application 层做 Order(实体) -> OrderResponse(DTO) 的显式映射
```

---

## 2. 分层与依赖方向

分层的意义不是"多写几个文件夹"，而是**约束依赖方向、隔离变化**。层与层之间只允许单向依赖。

### 2.1 标准分层与职责

```
接入层 Controller/Handler   接收请求、参数校验、鉴权、DTO<->领域对象转换、组织响应
        │  (只调用下层，不写业务规则)
        ▼
应用服务层 Service           用例编排：事务边界、调用领域对象、协调多个仓储/外部服务
        │
        ▼
领域层 Domain               核心业务规则、实体、值对象、领域服务（纯业务，不知道 HTTP/DB）
        │
        ▼
基础设施层 Infrastructure    仓储实现、外部 API 客户端、消息、缓存（可替换的"外设"）
```

### 2.2 依赖必须单向，禁止反向与跨层穿透

- 上层依赖下层，**下层永不依赖上层**（领域层不 import controller、不知道 web 框架）。
- 禁止**跨层穿透**：controller 不直接调 repository 越过 service；service 不直接拼 SQL 越过仓储抽象。
- 依赖倒置：领域层定义仓储**接口**，基础设施层提供**实现**，让核心业务不被 DB/框架绑架。

```text
// BAD：controller 直接查库 + 把业务规则写在接入层
class OrderController {
    fn create(req) {
        // 越过 service 直接摸数据库
        let user = db.query("SELECT * FROM users WHERE id=?", req.userId)
        // 业务规则（会员折扣、库存扣减）散落在 controller
        let price = req.price
        if user.level == "vip" { price = price * 0.9 }
        if req.qty > stock(req.itemId) { return error(400, "缺货") }
        db.exec("INSERT INTO orders ...")
        return ok()
    }
}

// GOOD：controller 只做接入，业务规则回到 service/domain
class OrderController {
    fn create(req: CreateOrderRequest) -> OrderResponse {
        let cmd = req.to_command()          // DTO -> 领域输入
        let order = order_service.place(cmd) // 用例编排在 service
        return OrderResponse::from(order)    // 领域对象 -> DTO
    }
}

class OrderService {                         // 应用层编排事务与协作
    fn place(cmd: PlaceOrderCommand) -> Order {
        let user = users.find(cmd.user_id)   // 经仓储接口，不碰 SQL
        inventory.reserve(cmd.item_id, cmd.qty) // 缺货在此抛领域错误
        let order = Order::new(user, cmd.items) // 折扣等规则在领域对象内
        return orders.save(order)
    }
}
```

### 2.3 业务规则的归属：只在 service/domain，绝不散落

业务规则（定价、状态流转、校验、权限判定）**必须**落在应用服务层或领域层。**绝不**允许出现在：controller/handler、DTO、util/helper、前端、SQL 里。判据：如果删掉某个 util 函数会导致一条业务规则消失，那这条规则就放错了地方。

### 2.4 贫血模型 vs 充血模型

- **贫血模型**：实体只有字段+getter/setter，规则全在 service。简单、上手快，但复杂领域下 service 会膨胀成"事务脚本"。
- **充血模型**：实体自带与其状态紧密相关的行为（`order.cancel()` 内部校验状态机），service 只做跨实体编排。领域复杂时更内聚、更防散落。
- 取舍：CRUD 为主、规则稀薄 → 贫血够用；领域规则多、状态流转复杂 → 充血（把"什么状态能做什么"关进实体）。**无论哪种，规则都不许漏到接入层。**

```text
// 充血：状态流转规则内聚在实体，非法转移无处发生
impl Order {
    fn cancel(&mut self) -> Result<()> {
        match self.status {
            Paid | Pending => { self.status = Cancelled; Ok(()) }
            Shipped        => Err("已发货不可取消"),
            _              => Err("非法状态转移"),
        }
    }
}
```

---

## 3. 分包 / 目录结构

### 3.1 feature-based vs layer-based

- **layer-based（按技术类型）**：`controllers/`、`services/`、`repositories/` 三大筐。小项目直观，但一改需求要横跨三个目录，且**目录不体现业务域**。
- **feature-based（按业务域）**：`order/`、`payment/`、`user/` 各自内聚。**推荐**：改一个业务只动一个目录，边界清晰，易于分工与拆分。
- 判据：**先按业务域（feature）切，域内再按层切**。当某业务域 5-20 人协作、文件众多时，域内可再用 layer 分。

### 3.2 目录树模板（按业务域拆）

```
src/
├─ shared/                  # 跨业务域复用：无业务的基础件
│  ├─ types/               # 通用类型、Result、错误基类
│  ├─ lib/                 # 纯工具（明确职责，不是杂物堆）
│  └─ platform/            # 日志、配置、DB 连接、HTTP 客户端封装
├─ order/                   # 业务域：订单
│  ├─ api/                 # controller / handler：接入层
│  ├─ service/             # 应用服务：用例编排
│  ├─ domain/              # 实体、值对象、领域服务、仓储接口
│  ├─ infra/               # 仓储实现、外部客户端
│  └─ mod.rs / index       # 该域对外公开 API（唯一出口）
├─ payment/                 # 业务域：支付（同构）
└─ user/                    # 业务域：用户（同构）
```

### 3.3 模块边界与包私有性，防循环依赖

- 每个业务域对外只暴露一个**公开出口**（barrel / mod / package-info），跨域引用只走公开出口，**禁止深入对方内部目录直接 import**。
- 域内实现细节用语言的可见性机制设为包私有/模块私有，别让内部对象漏出去被到处依赖。
- **循环依赖是设计缺陷**：`order` 依赖 `payment`，`payment` 又反过来依赖 `order`，说明边界划错或缺一个共享抽象。破解：把共享的类型/接口下沉到 `shared`，或用事件/接口解耦，让依赖重新变成单向的有向无环图。

```text
// BAD：跨域深层 import 对方内部，耦合成团
use crate::payment::infra::alipay_client::AlipayClient;

// GOOD：只依赖对方公开出口暴露的抽象
use crate::payment::PaymentGateway; // trait，从 payment 的公开出口导出
```

---

## 4. 反屎山硬规则（可度量红线）

这些是**硬性阈值**，不是建议。超限即触发拆分动作。数值是通用安全区间，团队可微调，但方向不变。

### 4.1 单文件行数 ≤ 400-500 行

超限意味着该文件承担了太多职责。拆分依据：把不同职责的类型/函数分到不同文件。

### 4.2 单函数行数 ≤ 50-80 行

一个函数应能一屏读完、只讲一件事。超长函数按"步骤边界"抽出子函数。

```text
// BAD：一个函数取数、校验、算价、落库、发通知，180 行全糊一起
fn handle_order(req) { /* ...180 行... */ }

// GOOD：编排 + 命名良好的子步骤，每步可读可测
fn handle_order(cmd) {
    let ctx = validate(cmd)?;
    let priced = price(ctx)?;
    let saved = persist(priced)?;
    notify(saved);
    saved
}
```

### 4.3 圈复杂度 ≤ 10-15

分支/循环嵌套过多的函数无法被测试覆盖也无法被理解。超阈值 → 用查表、多态、策略模式或早返回压平分支。

### 4.4 函数参数 ≤ 4 个，超了用参数对象

参数一多，调用点就靠位置传参、极易错位。三个以上强相关参数打包成一个具名对象/结构体。

```text
// BAD：位置参数一长串，调用点无法看出谁是谁
fn create_user(name, email, age, is_admin, is_verified, country, ref_code) { ... }
create_user("a", "b@x", 30, false, true, "CN", null) // 哪个 bool 是哪个？

// GOOD：参数对象，字段具名、可选值有默认、扩展不破坏签名
fn create_user(input: CreateUserInput) { ... }
create_user(CreateUserInput { name, email, is_admin: false, ... })
```

### 4.5 单一职责（SRP）：禁止 god class / god function

一个类/模块只有一个"变化的理由"。`OrderService` 同时管订单、库存、支付、短信 → 拆成各自的 service。判据：**描述这个类做什么时如果要用"并且/以及"连接多个不相关职责，就该拆。**

### 4.6 禁止 util/common/helper 变成黑洞

`util/` 不是垃圾场。判据：一个函数放进 `util` 前先问"它属于哪个业务域？"——属于计费就放 `billing/`，属于日期处理就放 `shared/lib/datetime`。**允许存在的通用工具必须无业务语义、职责单一、有清晰文件名**（`datetime.rs`、`money.rs`），而不是一个什么都往里塞的 `helpers.rs`。拆分方法：给现有 `utils.rs` 里的每个函数问归属，能归业务域的迁走，剩下的按主题（字符串/日期/数值）拆成命名清晰的小模块。

### 4.7 重复代码提取：DRY 的度

- 重复三次以上、且**语义相同**的逻辑 → 提取。
- 但别为"长得像"就强行合并——两段代码今天相同、明天因不同业务原因各自演化，强行 DRY 反而制造错误耦合（过早抽象比重复更糟）。判据：**它们是否因同一个原因而改变**。是 → 合并；否 → 保持独立。

### 4.8 嵌套深度 ≤ 3-4，用早返回/卫语句压平

```text
// BAD：金字塔嵌套，happy path 埋在最里层
fn pay(order) {
    if order != null {
        if order.status == Pending {
            if order.amount > 0 {
                if user.balance >= order.amount {
                    // 真正的逻辑在第 5 层
                }
            }
        }
    }
}

// GOOD：卫语句先排除异常，主逻辑回到顶层
fn pay(order) {
    guard order != null            else return err("订单为空")
    guard order.status == Pending  else return err("状态不可支付")
    guard order.amount > 0         else return err("金额非法")
    guard user.balance >= order.amount else return err("余额不足")
    // 主逻辑，不再缩进
}
```

---

## 5. 错误处理 / 契约 / 日志 的结构规范

### 5.1 统一错误模型与错误码分级

- 全项目一个错误基类型/枚举，携带：稳定错误码、面向用户的消息、面向排障的内部细节。
- 错误码分级：`4xx` 类=调用方问题（可预期，不必告警）；`5xx` 类=系统问题（需告警）；用前缀/区段区分业务域（`ORDER_*`、`PAY_*`）。
- **禁止**吞异常（`catch {}` 空处理）、禁止用返回 `null`/`-1` 表达失败并让调用方猜。

```text
// BAD：错误信息裸字符串，调用方无法据此分流，还吞掉了栈
try { pay() } catch (e) { return "失败" }

// GOOD：类型化错误 + 稳定码，调用方可按码处理，日志留细节
enum AppError { code: "PAY_INSUFFICIENT_BALANCE", userMsg: "余额不足", cause }
```

### 5.2 先定契约，再实现

写接口/模块前先定义**契约**（入参、出参、错误集合、幂等性、超时语义），再写实现与联调。契约集中为类型/schema，前后端/上下游共享同一份，杜绝字段名对不上、类型不一致的返工。

### 5.3 日志必须带请求上下文标识

- 每条日志携带贯穿一次请求的 `traceId`（及关键业务标识如 `orderId`），否则并发下日志无法拼成一条链路。
- 分级得当（DEBUG/INFO/WARN/ERROR），**禁止日志里打印密钥、令牌、完整身份证/卡号**等敏感信息。
- 结构化日志（键值对）优于拼字符串，便于检索与聚合。

```text
// BAD：无上下文，并发下无法定位是哪次请求
log("order created")

// GOOD：带 traceId + 结构化字段
log.info("order.created", traceId=ctx.traceId, orderId=order.id, amountCents=order.total)
```

---

## 6. 评审基线 / 常见失败模式

### 6.1 评审清单（逐项可勾选）

- [ ] **命名**：无无语义缩写；布尔有 is/has/can 前缀；时间字段 `xxxAt`+UTC；金额带单位/精度；集合复数。
- [ ] **分层**：无跨层穿透（controller 不直连 DB）；依赖单向；领域层不依赖框架。
- [ ] **业务规则归属**：规则在 service/domain，未散落在 controller/DTO/util/前端。
- [ ] **数据对象**：DTO/VO/Entity 分清，实体未裸传到接口边界；敏感字段不出边界。
- [ ] **分包**：按业务域分包；跨域只走公开出口；无循环依赖；无 util 黑洞。
- [ ] **体量红线**：文件 ≤500 行、函数 ≤80 行、圈复杂度 ≤15、参数 ≤4、嵌套 ≤4。
- [ ] **单一职责**：无 god class/function（描述职责不需要"并且"连接）。
- [ ] **错误**：统一错误模型+稳定错误码；无空吞异常；失败不靠 null/-1 表达。
- [ ] **契约**：接口先有契约再实现；前后端/上下游共享同一份类型。
- [ ] **日志**：带 traceId+业务标识；分级得当；无敏感信息；结构化。
- [ ] **测试**：关键分支有单测；改 bug 附带防回归测试。
- [ ] **变更安全**：破坏性变更有回滚说明与迁移方案。

### 6.2 常见失败模式（出现即打回）

- 业务规则散落在 controller、DTO、util、前端，删掉一个工具函数就丢一条规则。
- controller 直连数据库或直接拼 SQL，越过 service/仓储。
- 数据库实体一路裸传到接口返回，内部/敏感字段外泄。
- 无语义缩写命名（`u`/`amt`/`tmp`/`data2`），阅读需逐个逆向翻译。
- 金额用浮点、时间无时区无单位，埋下精度与跨时区 bug。
- god class / 千行文件 / 百行函数 / 五层嵌套，无人能安全改动。
- `utils.rs`/`common.rs` 成黑洞，什么都往里塞，无边界无归属。
- 循环依赖：两个模块互相 import，无法独立编译/测试/替换。
- 空吞异常、用 null/-1 表达失败，错误在系统里静默扩散。
- 日志无 traceId，并发排障时无法把日志拼成一次请求的链路。
- 只修当前 bug、不补防回归测试；破坏性变更无回滚方案。

---

**核心心法**：命名让代码自解释，分层让依赖可控，体量红线让复杂度不失控，业务规则集中让"改一处不炸全场"。这些规则彼此正交、可度量、跨栈通用——任何底座据此即可产出通过资深评审的工程结构。

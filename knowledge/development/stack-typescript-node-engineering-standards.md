---
id: stack-typescript-node-engineering-standards
title: TypeScript + Node.js 后端工程规范（商业级·分层·反屎山）
domain: development
category: 01-standards
difficulty: advanced
tags: [TypeScript, Node, Nest, Express, 分层, 分包, 命名规范, 反屎山, backend, DTO]
quality_score: 92
last_updated: 2026-07-12
---

# TypeScript + Node.js 后端工程规范（商业级·分层·反屎山）

> 本文是语言无关分层方法论在 **TypeScript + Node.js** 服务端（Nest / Express 风格）上的落地细则。商业级后端不是"路由里塞满逻辑能跑通"，而是**按 feature 分包、controller 薄、service 承载业务、repository 隔离数据、DTO 与 domain 分离、类型严格、错误结构化**。写接口前先定分层骨架，再填实现。route handler 里直接查库、`any` 满天飞、逻辑塞进 controller、把杂物丢进 `utils/`，都是不合格的。

## 0. 一句话原则

**依赖向内：controller 依赖 service，service 依赖 repository 抽象，domain 不依赖任何外设。HTTP、数据库、第三方 SDK 都是可替换的边缘，业务规则居核心。**

## 1. 分层模型与目录结构

```
HTTP 入口 Controller/Route ─▶ 应用服务 Service ─▶ 仓储 Repository(接口) ─▶ 数据库/ORM
        │(只做 HTTP↔DTO)          │(业务编排/事务)        │(唯一 SQL/ORM 出口)
        └─ 校验 DTO 入参           └─ 依赖注入拿依赖        └─ 返回 domain/entity
                                   └─ 调 domain 纯逻辑
```

- **Controller / Route handler**：只负责 HTTP 边界——解析请求、校验 DTO、调用 service、映射响应状态码。**不写业务逻辑、不碰数据库、不做跨实体编排。**
- **Service（应用服务）**：承载业务用例、编排多个 repository、管理事务边界、调用 domain 纯逻辑。业务的家在这里。
- **Repository**：数据访问的**唯一**出口，封装 ORM/SQL；对上暴露接口（`OrderRepository`），对下用具体实现（Prisma/TypeORM/knex）。service 依赖接口而非实现。
- **Domain / Entity**：领域实体与纯业务规则（计算、状态机、校验），无 IO、无框架依赖、可独立单测。
- **DTO**：跨越 HTTP 边界的数据形状（入参/出参），与 domain 实体**分离**（见 §2）。

按 **feature 分包**（不按技术类型堆大筐）。每个业务域自带全套分层：

```
src/
├─ modules/                       # 按业务域（feature）分包 ← 推荐
│  ├─ orders/
│  │  ├─ orders.controller.ts     # HTTP 边界，薄
│  │  ├─ orders.service.ts        # 业务用例编排
│  │  ├─ orders.repository.ts     # 数据访问唯一出口
│  │  ├─ dto/
│  │  │  ├─ create-order.dto.ts   # 入参 DTO + 校验
│  │  │  └─ order-response.dto.ts # 出参 DTO
│  │  ├─ entity/
│  │  │  └─ order.entity.ts       # domain 实体 + 业务规则
│  │  ├─ types.ts                 # 该域内部类型
│  │  └─ orders.module.ts         # Nest module 装配（Express 用 index.ts 组装）
│  ├─ payments/
│  └─ auth/
├─ shared/                        # 跨域复用：无业务的基础设施
│  ├─ errors/                     # AppError 体系
│  ├─ middleware/                 # 错误中间件、鉴权、日志
│  ├─ config/                     # 环境配置（校验后的强类型）
│  └─ lib/                        # 通用工具（有明确领域归属就别放这）
└─ main.ts                        # 应用装配入口
```

- 跨 feature 只通过对方模块公开出口引用，**禁止深层 import** 对方 repository 内部。
- Nest 项目用 module 边界；Express 项目按目录 + 显式组装函数，同样保持 feature 边界优先。
- 中型项目可用 feature + 少量共享层，但 feature 边界优先于技术分层。

## 2. 命名与类型规范

**命名约定（一致性本身就是可读性）：**

| 目标 | 约定 | 例 |
|---|---|---|
| 文件名 | kebab-case，带角色后缀 | `create-order.dto.ts`、`orders.service.ts` |
| 类 / 接口 / 类型 / 枚举 | PascalCase | `OrderService`、`OrderStatus` |
| 变量 / 函数 / 方法 | camelCase | `createOrder`、`totalAmount` |
| 常量 / 枚举成员 | UPPER_SNAKE_CASE | `MAX_RETRY`、`OrderStatus.PENDING` |
| 布尔字段 | is/has/can/should 前缀 | `isPaid`、`hasShipped`、`canCancel` |
| 时间字段 | 语义 + At/时区显式 | `createdAt`、`expiresAt`（存 UTC ISO） |
| 金额字段 | 最小单位 + 币种，禁浮点 | `amountCents: number`、`currency: 'USD'` |

- **接口不用 `I` 前缀**（`Order` 不是 `IOrder`）；`interface` 用于可扩展的对象契约，`type` 用于联合/交叉/工具类型。二者取舍：对象形状默认 `interface`，需要 union/mapped/条件类型时用 `type`。
- **DTO 与 domain/entity 分离**：DTO 是传输形状（可被外部污染、可选字段多），entity 是业务真相（不变量收紧）。二者用显式映射函数转换，不要图省事复用同一个类型贯穿全栈。
- **禁 `any`**：外部输入用 `unknown` 接住再收窄。开启 `strict`（含 `strictNullChecks`、`noImplicitAny`）。禁 non-null 断言 `!`（用收窄或显式判空）。导出函数写**显式返回类型**。

正例：

```typescript
// tsconfig.json: "strict": true, "noUncheckedIndexedAccess": true

export const OrderStatus = {
  PENDING: 'pending',
  PAID: 'paid',
  CANCELLED: 'cancelled',
} as const;
export type OrderStatus = (typeof OrderStatus)[keyof typeof OrderStatus];

// domain 实体：不变量收紧
export interface Order {
  readonly id: string;
  readonly amountCents: number;
  readonly currency: 'USD' | 'EUR';
  readonly status: OrderStatus;
  readonly createdAt: string; // UTC ISO
}

// DTO：传输形状，与 entity 分离
export interface CreateOrderDto {
  amountCents: number;
  currency: 'USD' | 'EUR';
}

// unknown 收窄，显式返回类型
function parseAmount(raw: unknown): number {
  if (typeof raw !== 'number' || !Number.isInteger(raw) || raw <= 0) {
    throw new ValidationError('amountCents must be a positive integer');
  }
  return raw;
}
```

反例：

```typescript
// 反例：any 泛滥、I 前缀、DTO 复用 entity、non-null 断言、隐式返回
interface IOrder { id; amount: any; created: string } // 隐式 any、字段无类型

function handle(body: any) {                    // any 入参
  const order = body!.order!;                   // non-null 断言链
  return order.amount * 1.1;                    // 浮点算金额，隐式返回类型
}
```

## 3. Service 层纪律

- **业务逻辑不写在 controller / route handler**：handler 只做 HTTP↔DTO 与状态码，业务下沉到 service。
- **依赖注入而非 `new` / 全局单例**：service 通过构造函数接收 repository/其他 service（Nest 用 DI 容器，Express 用手动组装/工厂），便于替换与测试。禁止在 service 内部 `new Repository()` 或引用模块级全局单例。
- **纯函数与副作用隔离**：计算/校验/派生用 domain 纯函数（无 IO），IO（查库、发消息、调外部 API）留在 service 边缘，事务边界显式。

正例：

```typescript
// controller：薄，只做边界
@Post()
async create(@Body() dto: CreateOrderDto): Promise<OrderResponseDto> {
  const order = await this.orders.placeOrder(dto);      // 业务全在 service
  return toResponseDto(order);
}

// service：DI 拿依赖，编排业务，调 domain 纯逻辑
export class OrderService {
  constructor(
    private readonly repo: OrderRepository,             // 依赖抽象，注入
    private readonly payments: PaymentGateway,
  ) {}

  async placeOrder(dto: CreateOrderDto): Promise<Order> {
    const order = createOrder(dto);                     // domain 纯函数，无 IO
    await this.repo.save(order);                        // IO 在边缘
    return order;
  }
}
```

反例：

```typescript
// 反例：逻辑塞进 handler、new 依赖、全局单例、handler 直接查库
app.post('/orders', async (req, res) => {
  const repo = new OrderRepository(globalDb);           // new + 全局单例
  if (req.body.amount > 0 && req.body.currency) {        // 业务判断写在 handler
    const row = await globalDb.query('INSERT ...');      // handler 直接碰库
    res.json(row);
  }
});
```

## 4. 反屎山硬规则

违反下列任一条即判不合格，需拆分/重构：

- **单文件 ≤ 300–400 行**：超了说明职责过载，按子域拆。
- **单函数 ≤ 50 行**：超了抽子函数或下沉 domain 逻辑。
- **圈复杂度 ≤ 10**：分支/循环过多就用早返回、策略表、状态机拆。
- **参数 ≤ 4**：超过用 options 对象（命名参数），避免"布尔陷阱"位置参数。
- **禁 god module**：一个 module 什么都管即拆按业务域。
- **禁 `utils/` / `helpers/` 黑洞**：有明确领域归属的 helper 放进对应 feature；`shared/lib` 只放真正通用、无业务的原语。
- **禁 barrel 循环依赖**：`index.ts` 只重导出，不产生 A↔B 环；跨 feature 走公开出口。
- **嵌套 ≤ 3 层**：用 **早返回**（guard clause）压平，别写金字塔 if。

正例（options 对象 + 早返回）：

```typescript
interface SendEmailOptions {
  to: string;
  subject: string;
  body: string;
  cc?: string[];
  replyTo?: string;
}

function sendEmail(opts: SendEmailOptions): Promise<void> { /* ... */ }

function priceFor(order: Order): number {
  if (order.status === 'cancelled') return 0;   // 早返回，压平嵌套
  if (order.currency !== 'USD') throw new ValidationError('unsupported currency');
  return order.amountCents;
}
```

反例（布尔陷阱 + 深嵌套）：

```typescript
// 反例：位置参数（谁记得第 4 个 true 是啥）、5 层嵌套
function send(to: string, sub: string, body: string, html: boolean, urgent: boolean) {}
send('a@x.com', 'Hi', '...', true, false);       // 布尔陷阱

function price(order: Order): number {
  if (order) {                                    // 金字塔
    if (order.status !== 'cancelled') {
      if (order.currency === 'USD') {
        if (order.amountCents > 0) {
          return order.amountCents;
        }
      }
    }
  }
  return 0;
}
```

## 5. 异步与错误处理

- **async/await 一致**：不要混用裸 `.then()` 链与 `await`；并发用 `Promise.all` 而非串行 await。
- **禁未处理 promise**：每个 `Promise` 要么 `await`、要么显式 `.catch()`、要么 `void` 标注有意 fire-and-forget；开 `no-floating-promises` lint。
- **错误用 `Error` 子类或 Result，禁 `throw` 字符串**：定义 `AppError` 体系带类型码，便于中间件分类映射 HTTP 状态。
- **统一错误中间件**：一处集中把 domain 错误映射到状态码 + 结构化响应；handler 里不散落 try/catch 拼 JSON。
- **不吞错误**：禁空 `catch {}`；至少记录并重抛或转成已知错误。
- **超时 / 重试 / 取消**：外部调用设超时，幂等操作有限重试（指数退避），长任务支持 `AbortController` 取消。

正例：

```typescript
export class AppError extends Error {
  constructor(message: string, readonly code: string, readonly status: number) {
    super(message);
    this.name = new.target.name;
  }
}
export class ValidationError extends AppError {
  constructor(msg: string) { super(msg, 'VALIDATION', 422); }
}
export class NotFoundError extends AppError {
  constructor(msg: string) { super(msg, 'NOT_FOUND', 404); }
}

// 带超时/取消的外部调用
async function fetchQuote(id: string): Promise<Quote> {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), 3000);
  try {
    const res = await fetch(`/quotes/${id}`, { signal: ctrl.signal });
    if (!res.ok) throw new AppError('quote fetch failed', 'UPSTREAM', 502);
    return (await res.json()) as Quote;
  } finally {
    clearTimeout(timer);
  }
}

// 统一错误中间件：一处映射，handler 不拼 JSON
export function errorMiddleware(err: unknown, _req, res, _next) {
  if (err instanceof AppError) {
    return res.status(err.status).json({ code: err.code, message: err.message });
  }
  logger.error(err);                              // 未知错误记录后回 500，不泄露栈
  return res.status(500).json({ code: 'INTERNAL', message: 'internal error' });
}
```

反例：

```typescript
// 反例：throw 字符串、空 catch 吞错、未处理 promise、handler 里拼 JSON
async function pay(id: string) {
  saveAudit(id);                                  // 未 await 的 promise，静默丢失
  try {
    if (!id) throw 'missing id';                   // throw 字符串
    await charge(id);
  } catch (e) {
    // 空 catch：错误被吞，调用方以为成功
  }
  return { ok: true };                            // 无超时、无取消、状态不真实
}
```

## 6. 数据与校验

- **边界处 DTO 校验**：请求入口用 schema 校验（`zod` 或 `class-validator`），把 `unknown` 转成强类型 DTO；校验失败抛 `ValidationError`。
- **禁裸 `any` 入库**：写库前经 DTO→entity 映射，字段类型/不变量已收紧。
- **参数化查询防注入**：一律用参数占位符或 ORM 查询构造器，**禁字符串拼接 SQL**。
- **N+1 意识**：列表关联数据用 join / `include` / DataLoader 批量取，禁循环里逐条查库。

正例：

```typescript
import { z } from 'zod';

const CreateOrderSchema = z.object({
  amountCents: z.number().int().positive(),
  currency: z.enum(['USD', 'EUR']),
});
type CreateOrderDto = z.infer<typeof CreateOrderSchema>;

function parseCreateOrder(raw: unknown): CreateOrderDto {
  const r = CreateOrderSchema.safeParse(raw);
  if (!r.success) throw new ValidationError(r.error.message);
  return r.data;                                  // unknown → 强类型
}

// 参数化查询 + 批量取避免 N+1
const orders = await db.query(
  'SELECT * FROM orders WHERE user_id = $1',       // 占位符，非拼接
  [userId],
);
const users = await repo.findByIds(orders.map((o) => o.userId)); // 批量，非循环查
```

反例：

```typescript
// 反例：无校验直接用、字符串拼 SQL（注入）、循环里查库（N+1）
async function create(raw: any) {                 // any 未校验
  const sql = `INSERT INTO orders(amount) VALUES('${raw.amount}')`; // 注入
  await db.exec(sql);
  for (const o of orders) {
    o.user = await db.query(`SELECT * FROM users WHERE id=${o.userId}`); // N+1 + 注入
  }
}
```

## 7. 评审清单（可勾选）

- [ ] 分层清晰：controller 薄（只做 HTTP↔DTO），业务在 service，数据访问只在 repository，domain 无框架/IO 依赖。
- [ ] 按 feature 分包（`modules/<feature>/{controller,service,repository,dto,entity}`），跨域不深层 import。
- [ ] 命名一致：文件 kebab-case、类型 PascalCase、常量 UPPER_SNAKE；布尔 is/has、时间 At（UTC）、金额最小单位整数。
- [ ] 类型严格：`strict` 开启，无 `any`（用 `unknown` 收窄）、无 non-null `!`、导出函数有显式返回类型；DTO 与 entity 分离。
- [ ] Service 用依赖注入，不 `new` 依赖、不用全局单例；纯逻辑与副作用隔离。
- [ ] 反屎山达标：文件 ≤400 行、函数 ≤50 行、圈复杂度 ≤10、参数 ≤4（用 options 对象）、嵌套 ≤3（早返回）；无 god module、无 utils 黑洞、无 barrel 环。
- [ ] 异步安全：async/await 一致、无未处理 promise、错误用 `Error` 子类、统一错误中间件、不吞错误、外部调用有超时/重试/取消。
- [ ] 数据安全：入口 DTO 校验（zod/class-validator）、无裸 any 入库、参数化查询、无 N+1。

---
**要点回顾**：controller 薄、service 厚、repository 隔离、domain 纯；DTO≠entity；类型 strict 无 any；错误结构化 + 统一中间件；边界校验 + 参数化查询。这套结构让新增一个业务域是"复制一个 module 骨架填实现"，而非在几千行的 route 文件里继续叠屎山。

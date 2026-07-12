---
id: stack-python-engineering-standards
title: Python 工程规范（商业级·分层·反屎山）
domain: development
category: 01-standards
difficulty: advanced
tags: [Python, FastAPI, Django, 分层, 分包, 命名规范, 类型注解, 反屎山, backend]
quality_score: 92
last_updated: 2026-07-12
---

# Python 工程规范（商业级·分层·反屎山）

> 框架无关（FastAPI / Django / Flask 通用）的硬性工程标准，聚焦 Python 后端服务。商业级后端不是"路由里塞一坨能返回 200"，而是**按业务域分包、请求处理与业务逻辑分层、数据访问统一隔离、全量类型注解、资源与异常显式管理**。写接口前先定骨架，再填实现。router 里写业务、裸 `dict` 满天飞、把所有东西丢进 `utils.py`，都是不合格的。语言无关的通用方法论（SOLID、圈复杂度理论等）另有专文，本文只讲 Python 落地的技术细节。

## 0. 一句话原则

**按业务域分包（feature not layer-folder），请求→业务→数据三层分明，依赖向内：router 依赖 service、service 依赖 repository，网络/ORM/外部 API 是可替换外设；一切公开边界都带类型注解。**

## 1. 分层模型与包结构

```
HTTP 入口 router/api ─▶ 业务编排 service ─▶ 数据访问 repository ─▶ domain/model(实体)
      │(Pydantic schema 校验入参/出参)          │(ORM / 外部 client)
      └── 只做：解析请求、鉴权、调 service、组装响应；不写业务
```

- **router / api（HTTP 边界层）**：只负责解析与校验请求（Pydantic schema）、鉴权、调用 service、组装 HTTP 响应与状态码。**一行业务逻辑都不写**。
- **service（业务编排层）**：承载业务规则、事务边界、跨 repository 编排、领域校验。不感知 HTTP（没有 `Request`/`Response`），也不直接写 SQL。
- **repository（数据访问层）**：与 ORM / 数据库 / 外部 API 通信的**唯一**出口，返回 domain 对象或实体，屏蔽存储细节。service 不裸调 `session.query`。
- **domain / model（领域实体）**：业务实体与不变量，ORM model 或纯 dataclass。

**按业务域（feature）分包，不按技术层建三个大筐。** 不要在项目根建 `routers/`、`services/`、`models/` 把全项目同类堆一起（改一个需求要翻三处）。目录树：

```
app/
├─ main.py                    # 应用装配：create_app、路由挂载、中间件、生命周期
├─ core/                      # 框架级基础设施（非业务）：config、db session、security、依赖
│  ├─ config.py              # Settings（pydantic-settings，读环境变量）
│  ├─ db.py                  # engine / SessionLocal / get_session 依赖
│  └─ exceptions.py          # 自定义异常层次 + 统一异常处理器
├─ orders/                    # 业务域：订单
│  ├─ __init__.py            # 只导出对外公开符号，界定包边界
│  ├─ router.py             # APIRouter，HTTP 端点，调 service
│  ├─ service.py            # OrderService，业务逻辑
│  ├─ repository.py         # OrderRepository，数据访问
│  ├─ schemas.py            # Pydantic DTO：OrderCreate / OrderOut（入出参）
│  └─ models.py             # ORM 实体：Order（数据库映射）
├─ billing/                   # 业务域：计费（与 orders 平级）
└─ shared/                    # 跨域复用：无业务的纯技术件（分页、时间、类型别名）
```

- 跨业务域只通过对方 `__init__.py`（或 `service` 公开方法）引用，**禁止深层 import** 对方 `repository`/`models` 内部实现。
- `__init__.py` 是包边界声明：`from .service import OrderService`，对外暴露的就这些；未导出的即私有。
- **禁止 `utils.py` / `common.py` 黑洞**。和计费相关的 helper 放进 `billing/`，通用的按语义命名（`shared/pagination.py`、`shared/datetime.py`），清晰文件名本身就是架构。

正例（router 只做边界，业务在 service）：

```python
# orders/router.py  —— GOOD：薄边界层
from fastapi import APIRouter, Depends, status
from .schemas import OrderCreate, OrderOut
from .service import OrderService, get_order_service

router = APIRouter(prefix="/orders", tags=["orders"])

@router.post("", response_model=OrderOut, status_code=status.HTTP_201_CREATED)
async def create_order(
    payload: OrderCreate,
    service: OrderService = Depends(get_order_service),
) -> OrderOut:
    order = await service.place_order(payload)   # 业务全在 service
    return OrderOut.model_validate(order)
```

反例（router 里写业务 + 裸查库 + 无分层）：

```python
# orders/router.py  —— BAD：router 变成了一坨全能函数
@router.post("/orders")
async def create_order(data: dict, db=Depends(get_db)):   # 裸 dict，无 schema
    if data["amount"] <= 0:                                # 业务规则塞在 HTTP 层
        return {"err": "bad"}                              # 手拼错误、无状态码
    user = db.query(User).filter_by(id=data["uid"]).first()  # router 直接查库
    if user.balance < data["amount"]:                     # 业务规则又一条
        return {"err": "no money"}
    order = Order(uid=data["uid"], amount=data["amount"])  # 边界层直接操作 ORM
    db.add(order); db.commit()
    return {"id": order.id}
```

## 2. 命名规范

| 种类 | 约定 | 示例 |
|---|---|---|
| 模块 / 包 | 小写下划线 `snake_case` | `order_service.py`、`billing/` |
| 类 | `PascalCase` | `class OrderService`、`class OrderCreate` |
| 函数 / 方法 / 变量 | `snake_case` | `def place_order()`、`total_amount` |
| 常量 / 模块级配置 | `UPPER_SNAKE_CASE` | `MAX_RETRY = 3`、`DEFAULT_PAGE_SIZE = 20` |
| 私有实现 | 前缀 `_` | `def _calc_discount()`、`self._session` |
| 类型别名 | `PascalCase` | `OrderId = NewType("OrderId", int)` |

- **布尔字段**用谓词前缀，读起来是"是不是/能不能"：`is_active`、`has_paid`、`can_refund`、`should_notify`。不要用 `flag`、`status_bool`、`active`（歧义）。
- **时间字段**带单位/时区语义，统一后缀 `_at`（时刻）/ `_on`（日期）：`created_at`、`expires_at`、`start_on`。存 UTC aware datetime，字段名不掺"本地"。
- **金额字段**带币种/单位，避免浮点：`amount_cents: int`（最小货币单位）或 `Decimal`；命名 `price_cents`、`total_amount`，禁止 `price: float` 表示钱。
- **集合命名**用复数：`orders`、`user_ids`；单个用单数：`order`。

**Pydantic schema（DTO）与 ORM model（entity）必须分离**，这是 Python 后端最常见的越界点：

```python
# orders/schemas.py  —— GOOD：DTO 是对外契约，可裁剪字段、可加校验
from decimal import Decimal
from pydantic import BaseModel, ConfigDict, Field

class OrderCreate(BaseModel):                 # 入参 DTO
    sku: str = Field(min_length=1, max_length=64)
    quantity: int = Field(gt=0, le=999)

class OrderOut(BaseModel):                     # 出参 DTO
    model_config = ConfigDict(from_attributes=True)   # 允许从 ORM 实例转换
    id: int
    total_amount: Decimal
    created_at: str                            # 对外可控形态，不直接吐 ORM

# orders/models.py  —— ORM 实体独立存在，不当作接口返回
from sqlalchemy.orm import Mapped, mapped_column
from app.core.db import Base

class Order(Base):
    __tablename__ = "orders"
    id: Mapped[int] = mapped_column(primary_key=True)
    sku: Mapped[str]
    quantity: Mapped[int]
    amount_cents: Mapped[int]
```

反例：直接把 ORM model 当请求体和响应体（暴露内部字段、耦合存储与接口、无法演进）。

```python
# BAD：ORM 实体既当入参又当出参，DB schema 一改接口就破
@router.post("/orders")
async def create(order: Order) -> Order:      # 用 ORM 类当 DTO
    ...
```

## 3. 类型注解纪律

- **全量 type hints**：所有函数参数、返回值、类属性都注解；返回类型显式写出（哪怕是 `-> None`）。
- **开启 `mypy --strict`**（或 pyright strict）作为 CI 门禁；`disallow_untyped_defs`、`disallow_any_generics`、`warn_return_any` 全开。
- **禁裸 `dict` / `Any` 在层间传递**：数据结构用 `dataclass` 或 Pydantic 建模。`dict[str, Any]` 只允许出现在最外层反序列化的瞬间，立刻转成模型。
- 用 `NewType` 给 id 类语义标量加类型（`OrderId`、`UserId`），避免"把 user_id 传进要 order_id 的位置"这类静默错。
- 用 `Protocol` 定义依赖接口（repository 的抽象），让 service 依赖协议而非具体类，便于替换与测试。

正例：

```python
from dataclasses import dataclass
from typing import Protocol

@dataclass(frozen=True)
class PricedItem:                              # 结构化，不用裸 dict
    sku: str
    unit_cents: int
    quantity: int

class OrderRepo(Protocol):                     # 依赖抽象协议
    async def add(self, order: "Order") -> "Order": ...
    async def get(self, order_id: int) -> "Order | None": ...

def line_total(item: PricedItem) -> int:       # 参数、返回类型都显式
    return item.unit_cents * item.quantity
```

反例（裸 dict + Any 到处传，类型系统形同虚设）：

```python
# BAD
def process(data):                             # 无注解
    items = data["items"]                      # dict[str, Any]，字段全靠猜
    total = 0
    for it in items:
        total += it["price"] * it["qty"]       # 拼错 key 运行时才炸
    return {"total": total}                    # 返回结构无契约
```

**可变默认参数陷阱**——Python 特有、必须规避：默认值只在函数定义时求值一次，可变默认参数会跨调用共享同一对象。

```python
# BAD：所有调用共享同一个 list，第二次调用带着上次的脏数据
def add_tag(tag: str, tags: list[str] = []) -> list[str]:
    tags.append(tag)
    return tags

# GOOD：默认用 None 哨兵，函数体内新建
def add_tag(tag: str, tags: list[str] | None = None) -> list[str]:
    tags = list(tags) if tags is not None else []
    tags.append(tag)
    return tags
```

## 4. Service 层纪律

- **业务逻辑不写在 router / view**：router 解析请求、调 service、组装响应；Django 里同理，view 薄、逻辑进 service/领域模块。
- **依赖注入而非全局单例**：用 FastAPI `Depends` 或构造函数显式传入 session / repository / 外部 client，不要在 service 内部 `import` 一个全局 `db` 或全局 client。可注入即可替换、可测试。
- **纯函数与 IO 隔离**：把计算/校验/派生做成不碰 IO 的纯函数（好测、可复用），把取数/写库/发请求的副作用留在 service 的编排层，两者不混在一个大方法里。
- **事务边界在 service**：一个业务用例是一个事务单元，由 service 开启/提交/回滚，不要让 repository 各自 commit 导致半截写入。

正例（依赖注入 + 纯逻辑与 IO 分离）：

```python
# orders/service.py  —— GOOD
from decimal import Decimal
from .repository import OrderRepository
from .schemas import OrderCreate

def compute_total(unit_cents: int, quantity: int, discount: Decimal) -> int:
    # 纯函数：无 IO，可独立单测
    gross = unit_cents * quantity
    return int(gross * (Decimal(1) - discount))

class OrderService:
    def __init__(self, repo: OrderRepository) -> None:
        self._repo = repo                      # 依赖注入，不 import 全局

    async def place_order(self, payload: OrderCreate) -> "Order":
        unit = await self._repo.unit_price(payload.sku)   # IO 在编排层
        total = compute_total(unit, payload.quantity, Decimal("0.1"))  # 纯计算
        return await self._repo.add_order(payload.sku, payload.quantity, total)

def get_order_service(repo: OrderRepository = Depends(get_order_repo)) -> OrderService:
    return OrderService(repo)                   # FastAPI 依赖装配
```

反例（全局单例 + 逻辑与 IO 缠死）：

```python
# BAD：模块级全局 db，无法替换/测试；计算与写库缠在一起
from app.core.db import db_session as db       # 全局单例

class OrderService:
    def place_order(self, payload):
        row = db.execute("SELECT price FROM sku ...").first()  # service 直接写 SQL
        total = row.price * payload["quantity"] * 0.9          # 魔法数 + 裸 dict
        db.execute("INSERT INTO orders ...")                   # IO 与逻辑混一坨
        db.commit()
```

## 5. 反屎山硬规则

出现即扣分，评审直接打回：

- **单文件 ≤ 400 行**：超了按职责拆包（router / service / repository / schemas 分文件本身就在帮你控制体量）。
- **单函数 ≤ 50 行**：一个函数只做一件事；超了抽子函数。
- **圈复杂度 ≤ 10**：分支/循环/布尔运算符太多就拆；用 `ruff`（`C901`）或 `radon` 卡阈值。
- **参数 ≤ 5**：再多就用 `dataclass` / Pydantic schema 打包成一个参数对象。
- **禁 god class**：一个类塞几十个方法、管订单又管发邮件又管对账——按职责拆成多个 service。
- **禁 `utils` / `common` / `helpers` 黑洞**：所有函数按语义归到业务域或命名清晰的技术模块。
- **禁 `from x import *`**：污染命名空间、遮蔽来源、`__all__` 之外的符号泄漏；一律显式导入。
- **嵌套 ≤ 3 层**：用早返回 / 卫语句（guard clause）拍平深层 `if`。
- **禁在循环里查库（N+1）**：批量取数用一次 `IN` 查询 / `join` / `selectinload`，不要循环里逐条 `get`。

卫语句拍平嵌套（GOOD vs BAD）：

```python
# BAD：金字塔嵌套，5 层深
def refund(order):
    if order is not None:
        if order.is_paid:
            if not order.is_refunded:
                if order.amount_cents > 0:
                    do_refund(order)
                    return True
    return False

# GOOD：卫语句早返回，主逻辑贴左边
def refund(order: "Order | None") -> bool:
    if order is None:
        return False
    if not order.is_paid or order.is_refunded:
        return False
    if order.amount_cents <= 0:
        return False
    do_refund(order)
    return True
```

消除 N+1（GOOD vs BAD）：

```python
# BAD：N 次查库
orders = await repo.list_orders(user_id)
for o in orders:
    o.user = await repo.get_user(o.user_id)    # 每条一次往返

# GOOD：一次批量取，内存组装
orders = await repo.list_orders(user_id)
user_ids = {o.user_id for o in orders}
users = await repo.get_users(user_ids)         # 单次 IN 查询
by_id = {u.id: u for u in users}
for o in orders:
    o.user = by_id[o.user_id]
```

参数过多改用结构体：

```python
# BAD：7 个位置参数，调用处全靠数位置
def create_order(sku, qty, uid, coupon, address, note, currency): ...

# GOOD：打包成 schema/dataclass，一个参数、带校验、可演进
def create_order(cmd: CreateOrderCommand) -> Order: ...
```

## 6. 错误与资源管理

- **自定义异常层次**：建一个领域异常基类，业务错误继承它，再由统一处理器映射到 HTTP 状态码。不要满地 `raise Exception("...")` 或 `HTTPException` 直接从 service 抛（service 不该感知 HTTP）。
- **不裸 `except:` / 不宽 `except Exception:` 吞错**：只捕获你能处理的具体异常；捕获后要么恢复、要么带上下文重抛（`raise ... from e`），绝不静默 `pass`。
- **用 `with` 管理资源**：文件、数据库 session、锁、外部连接一律上下文管理器，保证异常路径也释放。async 用 `async with`。
- **统一异常处理器**：在应用入口注册 exception handler，把领域异常翻译成规范错误响应，router 里不写重复 try/except。
- **async 一致性**：async 函数里不调阻塞 IO（`requests`、`time.sleep`、同步 DB 驱动）——会阻塞事件循环；用异步库（`httpx.AsyncClient`、`asyncio.sleep`、async DB 驱动），或把阻塞调用丢进 `run_in_executor`。不要 sync/async 混用制造隐性阻塞。

正例（异常层次 + 上下文管理 + 精准捕获）：

```python
# core/exceptions.py  —— GOOD：领域异常，不感知 HTTP
class DomainError(Exception):
    """业务错误基类。"""

class InsufficientBalance(DomainError):
    def __init__(self, need: int, have: int) -> None:
        super().__init__(f"need {need} have {have}")
        self.need, self.have = need, have

# main.py  —— 统一处理器，一处翻译成 HTTP
from fastapi import Request
from fastapi.responses import JSONResponse

@app.exception_handler(InsufficientBalance)
async def _balance_handler(_: Request, exc: InsufficientBalance) -> JSONResponse:
    return JSONResponse(status_code=402, content={"error": "insufficient_balance"})

# service：精准 raise、资源用 async with、失败带因重抛
async def charge(self, user_id: int, cents: int) -> None:
    async with self._uow() as uow:             # 事务/session 上下文管理
        user = await uow.users.get(user_id)
        if user.balance_cents < cents:
            raise InsufficientBalance(cents, user.balance_cents)
        try:
            await self._gateway.debit(user_id, cents)
        except GatewayTimeout as e:
            raise DomainError("charge failed") from e   # 带因重抛，不吞
```

反例（裸捕获吞错 + 手动开关资源 + 阻塞混用）：

```python
# BAD
async def charge(user_id, cents):
    conn = open_conn()                          # 手动开，异常就泄漏
    try:
        r = requests.post(url, json=...)        # async 里用同步 requests，阻塞事件循环
        time.sleep(1)                           # 同步 sleep 阻塞整个 loop
    except:                                      # 裸 except 吞掉一切，连 KeyboardInterrupt
        pass                                     # 静默失败，线上无从排查
    conn.close()                                 # 异常路径根本走不到这
```

## 7. 评审清单（写完后逐项自检）

- [ ] 分层清晰：router 只做 HTTP 边界，业务在 service，数据访问在 repository；router 无业务、service 无 SQL/HTTP。
- [ ] 按业务域分包，每域自带 router/service/repository/schemas/models；`__init__` 界定边界，跨域不深层 import。
- [ ] 无 `utils`/`common` 黑洞；无 `from x import *`；文件名语义清晰。
- [ ] Pydantic DTO 与 ORM model 分离；ORM 实体不当请求体/响应体。
- [ ] 全量 type hints，返回类型显式；`mypy --strict` 通过；层间不传裸 `dict`/`Any`。
- [ ] 无可变默认参数（`def f(x=[])`）；id 类标量用 `NewType`；依赖用 `Protocol` 抽象。
- [ ] service 依赖注入而非全局单例；纯计算与 IO 分离；事务边界在 service。
- [ ] 单文件 ≤400 行、单函数 ≤50 行、圈复杂度 ≤10、参数 ≤5、嵌套 ≤3（卫语句拍平）。
- [ ] 无 god class；无循环内查库（N+1 用批量查询消除）。
- [ ] 自定义异常层次；无裸 `except:`/宽 `except Exception` 吞错；资源用 `with`/`async with`；统一异常处理器。
- [ ] async 一致：async 路径无同步阻塞 IO；命名规范（布尔谓词前缀、时间 `_at`、金额 `_cents`/`Decimal`）达标。

---
**参考（commercial-grade Python 后端共识）**：分层架构（HTTP 边界 / 业务编排 / 数据访问三层解耦）、依赖倒置（service 依赖 `Protocol` 而非具体实现）、DTO 与实体分离（接口契约独立于存储模型）、显式优于隐式与全量类型注解（`mypy --strict` 门禁）。

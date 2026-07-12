---
id: stack-go-engineering-standards
title: Go 工程规范（商业级·分层·反屎山）
domain: development
category: 01-standards
difficulty: advanced
tags: [Go, Golang, 分层, 分包, 命名规范, 反屎山, backend, error-handling]
quality_score: 92
last_updated: 2026-07-12
---

# Go 工程规范（商业级·分层·反屎山）

> 面向 Go 后端服务的硬性结构标准。商业级 Go 不是"能编译能跑"，而是**按业务域分包、关注点分层、接口定义在消费方、错误显式传播带上下文、并发有明确生命周期**。Go 语言层面的克制（无继承、显式 error、组合优于继承）会诱使新手把逻辑摊平成一坨 `main.go` 加一个 `util` 包——那正是屎山起点。写服务前先定包边界与依赖方向，再填实现。本篇是 Go 栈特定的落地细则；语言无关的分层方法论另有专篇。

## 0. 一句话原则

**按业务域（domain）分包而非按技术类型，依赖单向向内：transport 依赖 usecase，usecase 依赖 domain，domain 不依赖任何人；接口在使用方定义、实现在提供方；error 一路显式 `%w` 包装上抛，`context.Context` 一路首参透传。**

## 1. 项目布局与分层

四层职责，依赖方向从外向内，内层不知道外层存在：

```
transport / handler   ← HTTP/gRPC/CLI 适配，解析请求、调用 usecase、编码响应
        │  依赖 ▼
service / usecase      ← 业务编排：事务、权限、跨仓储协作，无框架细节
        │  依赖 ▼
repository            ← 数据访问的唯一出口（DB/缓存/外部 API），接口在 usecase 侧定义
        │  依赖 ▼
domain               ← 实体、值对象、领域规则；纯 Go，不 import 上面任何层
```

标准布局的取舍（不要教条照搬 `cmd/ internal/ pkg/` 全套）：

- `cmd/<binary>/main.go`：只做装配——读配置、连 DB、构造依赖、启动服务，**不写业务**。一个可执行文件一个子目录。
- `internal/`：**默认放这里**。Go 编译器强制 `internal/` 内的包只能被同一父目录下的代码 import，天然锁死 API 边界，防止别人依赖你的实现细节。商业服务几乎所有业务代码都该在 `internal/`。
- `pkg/`：**只有**当你确实要对外提供可被其它仓库 import 的稳定库时才用。给内部服务硬套 `pkg/` 是 cargo-cult——它把本该私有的东西公开了。存疑就放 `internal/`。

按业务域分包（推荐）而非按技术类型分包：

```
# GOOD：按业务域分包，改一个需求只动一个目录
internal/
├─ order/
│  ├─ handler.go        # OrderHandler：HTTP 适配
│  ├─ service.go        # OrderService：业务编排
│  ├─ repository.go     # OrderRepository 接口 + 实现
│  └─ order.go          # Order 实体与领域规则
├─ payment/
│  ├─ handler.go
│  ├─ service.go
│  └─ payment.go
└─ platform/            # 跨域共享的技术设施（db 连接、日志、配置装配）
   ├─ postgres/
   └─ config/
```

```
# BAD：按技术类型分包，加一个订单字段要翻遍四个目录，且极易循环 import
internal/
├─ handlers/     # 所有 handler 堆一起
├─ services/     # 所有 service 堆一起
├─ models/       # 所有 model 堆一起
└─ util/         # 万物垃圾场（见下）
```

**Go 尤其忌讳 `util`/`common` 包黑洞。** 这类包无内聚语义、被所有包 import，很快变成谁都 import 谁的循环依赖源头，且名字什么信息都不传达。有内聚的东西按它的语义命名并归位：时间处理叫 `timeutil` 或直接进 `order` 里，钱相关的 helper 进 `money` 包。宁可多几个小而精准的包，不要一个 `util` 大筐。

## 2. 命名规范

包名是 Go 命名的核心，因为它是每个导出符号的前缀：

- 包名：**短、全小写、无下划线、无复数、语义化**。`order` 不是 `orders`/`orderService`/`order_pkg`。禁止无语义包名 `util`/`common`/`base`/`misc`/`helpers`/`shared`。
- 导出性即可见性：**首字母大写=导出（public），小写=包内私有**。没有 `public`/`private` 关键字，大小写就是访问控制。默认小写，只在真正需要跨包用时才大写。
- 避免 stutter（结巴）：包名已是前缀，符号名别重复它。

```go
// GOOD：调用处读作 http.Server / order.Service，干净
package order
type Service struct{ /* ... */ }
func New(repo Repository) *Service { /* ... */ }

// BAD：调用处读作 order.OrderService，包名结巴
package order
type OrderService struct{ /* ... */ }
func NewOrderService(repo OrderRepository) *OrderService { /* ... */ }
```

- 接口命名：**单方法接口用 `-er` 后缀**（`Reader`/`Notifier`/`Validator`）；接口要小。
- 错误变量：哨兵错误用 `Err` 前缀导出（`ErrNotFound`）；错误类型用 `Error` 后缀（`ValidationError`）。
- getter 不加 `Get`：`u.Name()` 而非 `u.GetName()`；只有真正带 IO/计算副作用的才用动词。setter 保留 `Set`。

```go
// GOOD
func (u *User) Name() string { return u.name }
var ErrOrderNotFound = errors.New("order not found")

// BAD
func (u *User) GetName() string { return u.name }  // 多余的 Get
var OrderNotFoundError = errors.New("order not found")  // 前后缀习惯反了
```

## 3. 接口与依赖

**接口定义在消费方（consumer-defined），不在实现方。** 谁用谁定义自己需要的最小接口，实现方只管返回具体类型。这样接口天然只暴露被真正用到的方法，且不会为了"抽象"而抽象。

```go
// GOOD：usecase 声明它需要什么，只声明用到的方法
package order

type Repository interface {          // 定义在消费方 order 包
    Save(ctx context.Context, o *Order) error
    FindByID(ctx context.Context, id string) (*Order, error)
}

type Service struct{ repo Repository }
func New(repo Repository) *Service { return &Service{repo: repo} }  // 构造函数注入
```

```go
// BAD：在实现方定义"大而全"接口，把整个 DAO 表面都抽象出来
package postgres

type OrderRepository interface {  // 定义在实现方，消费方被迫依赖全部方法
    Save(context.Context, *Order) error
    FindByID(context.Context, string) (*Order, error)
    FindAll(context.Context) ([]*Order, error)
    UpdateStatus(context.Context, string, int) error
    // ...一堆消费方根本不用的方法
}
```

- 依赖通过**构造函数注入**（`New(deps...)`），不在函数内部 `new` 出依赖，才可测可替换。
- **禁止全局可变状态**：包级 `var db *sql.DB` / 全局单例 config 让测试无法隔离、并发不安全。依赖显式传入结构体字段。

```go
// BAD：全局可变状态，测试互相污染、无法并发替换
package db
var Conn *sql.DB                    // 全局
func GetUser(id string) *User { Conn.QueryRow(/* ... */) }

// GOOD：连接握在结构体里，构造时注入
type UserRepo struct{ db *sql.DB }
func NewUserRepo(db *sql.DB) *UserRepo { return &UserRepo{db: db} }
```

## 4. 错误处理纪律

- **每个可能失败的调用都显式返回 `error` 并处理**；不吞、不忽略。
- 向上传播时用 `fmt.Errorf("...: %w", err)` **包装并带上下文**，`%w` 保留错误链供 `errors.Is/As` 解包。
- 判定用 `errors.Is`（哨兵）/ `errors.As`（类型），**不要 `err.Error() == "..."` 字符串比较**。
- **禁止 panic 作流程控制**；panic 只用于真正不可恢复的程序 bug（如启动期配置缺失），且服务边界要 recover 兜底。
- `defer` 关闭资源；对 `Close` 的错误也要处理或显式记录。
- **禁止 `_ = f()` 静默丢弃 error**。

```go
// GOOD：包装带上下文、defer 关闭、用 errors.Is 判定
func (s *Service) Load(ctx context.Context, id string) (*Order, error) {
    o, err := s.repo.FindByID(ctx, id)
    if err != nil {
        if errors.Is(err, ErrOrderNotFound) {
            return nil, err                                  // 语义错误原样上抛
        }
        return nil, fmt.Errorf("load order %s: %w", id, err) // 其它错误包装带上下文
    }
    return o, nil
}

func writeReport(path string, r *Order) (err error) {
    f, err := os.Create(path)
    if err != nil {
        return fmt.Errorf("create report: %w", err)
    }
    defer func() {
        if cerr := f.Close(); cerr != nil && err == nil {
            err = fmt.Errorf("close report: %w", cerr)       // 关闭错误也不丢
        }
    }()
    return json.NewEncoder(f).Encode(r)
}
```

```go
// BAD：吞错误、丢上下文、字符串比错、panic 当流程
func (s *Service) Load(id string) *Order {
    o, err := s.repo.FindByID(id)
    if err != nil {
        if err.Error() == "not found" {   // 脆弱的字符串比较
            panic(err)                    // 用 panic 控流
        }
        return nil                        // 吞掉 error，调用方无从判断
    }
    _ = s.audit(o)                        // 静默丢弃 error
    return o
}
```

## 5. 反屎山硬规则

- 单文件 ≤ 400–500 行；超了按职责拆分（handler / service / repository 各自成文件）。
- 单函数 ≤ 60–80 行；圈复杂度 ≤ 10–15。逻辑分支膨胀就抽子函数。
- **参数超过 3–4 个用 struct 承载**（options/params 结构体），别排一长串位置参数。
- 禁 god package（一个包几千行什么都管）、禁 `util` 黑洞（见 §1）。
- **嵌套 ≤ 3 层**：用早返回/卫语句压平，Go 惯用 `if err != nil { return }` 先处理错误与边界，主逻辑留在最外层不缩进。
- **`context.Context` 作每个跨边界函数的第一个参数**（`ctx context.Context`），一路透传，用于取消、超时、请求级值。不要存进结构体，不要传 `nil`（用 `context.TODO()`）。

```go
// GOOD：卫语句早返回，主逻辑不缩进；参数用 struct；ctx 首参
type CreateOrderParams struct {
    UserID  string
    Items   []Item
    Coupon  string
}

func (s *Service) Create(ctx context.Context, p CreateOrderParams) (*Order, error) {
    if p.UserID == "" {
        return nil, ErrInvalidUser
    }
    if len(p.Items) == 0 {
        return nil, ErrEmptyCart
    }
    o, err := buildOrder(p)
    if err != nil {
        return nil, fmt.Errorf("build order: %w", err)
    }
    return o, s.repo.Save(ctx, o)
}
```

```go
// BAD：箭头型深嵌套、一长串位置参数、无 ctx
func (s *Service) Create(userID string, items []Item, coupon string, notify bool) (*Order, error) {
    if userID != "" {
        if len(items) > 0 {
            o, err := buildOrder(userID, items, coupon)
            if err == nil {
                if err := s.repo.Save(o); err == nil {   // 缩进越陷越深
                    return o, nil
                }
            }
        }
    }
    return nil, errors.New("failed")
}
```

## 6. 并发

Go 的 goroutine 便宜，但**每个 goroutine 都必须有明确的退出路径**，否则就是泄漏。

- 启动 goroutine 前先想清楚：谁在什么条件下让它退出？用 `ctx` 取消或 `channel` 关闭作退出信号。
- 用 `context` 传播取消/超时；阻塞的 select 必须带 `<-ctx.Done()` 分支。
- 等待一组 goroutine 用 `sync.WaitGroup`；共享可变状态用 `sync.Mutex`/`sync.RWMutex` 或改用 channel 通信。
- **一律带 `-race` 跑测试**（`go test -race ./...`）；CI 必须开 race detector。

```go
// GOOD：goroutine 受 ctx 控制，会退出；WaitGroup 收敛；无数据竞争
func fanOut(ctx context.Context, ids []string, fetch func(context.Context, string) error) error {
    var wg sync.WaitGroup
    errCh := make(chan error, len(ids))
    for _, id := range ids {
        wg.Add(1)
        go func(id string) {                 // 传参避免闭包捕获循环变量
            defer wg.Done()
            select {
            case <-ctx.Done():               // 取消时及时退出
                errCh <- ctx.Err()
            default:
                errCh <- fetch(ctx, id)
            }
        }(id)
    }
    wg.Wait()
    close(errCh)
    for err := range errCh {
        if err != nil {
            return err
        }
    }
    return nil
}
```

```go
// BAD：goroutine 无退出信号（泄漏）、闭包捕获循环变量、裸写共享 map（数据竞争）
func fanOut(ids []string, fetch func(string) Result) map[string]Result {
    out := map[string]Result{}
    for _, id := range ids {
        go func() {
            out[id] = fetch(id)   // 并发写 map=竞争；捕获 id；out 无锁；无人 Wait
        }()
    }
    return out                    // 立即返回，goroutine 结果丢失，且永不回收
}
```

## 7. 评审清单（可勾选）

- [ ] 按业务域分包（`internal/<domain>/`），非按 handlers/services/models 技术类型堆放。
- [ ] 无 `util`/`common`/`base`/`misc` 黑洞包；每个包名短、小写、单数、有语义。
- [ ] 依赖单向向内：transport → usecase → repository → domain，domain 不 import 上层。
- [ ] `cmd/*/main.go` 只做装配；业务在 `internal/`；`pkg/` 仅用于真正对外的稳定库。
- [ ] 接口定义在消费方且小（用到几个方法定义几个），实现方返回具体类型。
- [ ] 依赖经构造函数注入；无全局可变状态、无包级单例连接/配置。
- [ ] 符号名无 stutter（`order.Service` 非 `order.OrderService`）；getter 不带 `Get`；哨兵错误 `Err` 前缀。
- [ ] error 全部显式处理，`%w` 包装带上下文；判定用 `errors.Is/As`，无字符串比错。
- [ ] 无 `_ = f()` 丢弃 error；资源 `defer` 关闭且关闭错误不吞；panic 不作流程控制。
- [ ] 单文件 ≤ ~500 行、单函数 ≤ ~80 行、嵌套 ≤ 3；卫语句早返回；多参数收进 struct。
- [ ] `context.Context` 作跨边界函数首参并透传，不入结构体、不传 nil。
- [ ] 每个 goroutine 有明确退出路径（ctx/channel）；共享状态加锁或用 channel；CI 开 `-race`。

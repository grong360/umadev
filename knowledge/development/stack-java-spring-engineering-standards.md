---
id: stack-java-spring-engineering-standards
title: Java + Spring Boot 工程规范（商业级·分层·反屎山）
domain: development
category: 01-standards
difficulty: advanced
tags: [Java, Spring, SpringBoot, 分层, 分包, 命名规范, DDD, MyBatis, JPA, 反屎山, backend]
quality_score: 92
last_updated: 2026-07-12
---

# Java + Spring Boot 工程规范（商业级·分层·反屎山）

> 面向 Spring Boot / jeecg-boot 技术栈的硬性结构标准。商业级后端不是"Controller 里写完 SQL 能跑就行"，而是**严格分层、职责单向依赖、业务逻辑只在 Service/Domain、数据库对象绝不裸奔到前端、单类单方法有体积上限**。写第一个 `@RestController` 前先定好包骨架，再填实现。这是 STACK 层的具体落地；语言无关的方法论在同目录另一份文件里，此处只讲 Java/Spring 的可执行细节。

## 0. 一句话原则

**请求自外向内穿过 Controller → Service → Mapper/Repository，依赖永远单向；表结构（Entity）不出 Service 层，出口是 DTO/VO；每一层只做自己那一件事。**

## 1. 分层模型与依赖方向

```
HTTP 请求
  └▶ Controller        参数接收 + @Valid 校验 + 编排调用，不写业务
       └▶ Service(接口)  业务规则、事务边界、领域编排的唯一归属
            └▶ ServiceImpl 实现，@Transactional 挂这里
                 └▶ Mapper/Repository  纯数据访问，不写业务判断
                      └▶ DB
  Entity ──(仅在 Service 内部流转)── Convert ──▶ VO/DTO ──▶ 返回前端
```

- **单向依赖**：Controller 依赖 Service 接口，Service 依赖 Mapper；反向禁止（Mapper 不得回调 Service，Service 不得 import Controller）。
- **面向接口**：Service 拆 `XxxService`（接口）+ `XxxServiceImpl`（实现），Controller 只注入接口，便于替换与测试。Mapper/Repository 是接口本身，无需再包一层。
- **依赖注入用构造器**，配合 `final` 字段，禁止 `@Autowired` 字段注入（不可测、隐藏依赖、易出空指针）。

正例（构造器注入 + 面向接口）：

```java
@RestController
@RequestMapping("/api/orders")
public class OrderController {
    private final OrderService orderService;   // 依赖接口，final，构造器注入

    public OrderController(OrderService orderService) {
        this.orderService = orderService;
    }
}
```

反例（字段注入 + 依赖实现类）：

```java
@RestController
public class OrderController {
    @Autowired
    private OrderServiceImpl orderService;   // 依赖具体实现、字段注入，不可测、耦合死
}
```

## 2. 包结构（feature-based，按业务域分包）

**默认按业务域（feature）分包，不按技术类型把全项目 Controller/Service 堆成三大筐。** 改一个"订单"需求应该只动 `order/` 一个目录，而不是翻遍 `controller/`、`service/`、`mapper/` 三处。

单模块工程标准骨架：

```
com.example.app
├─ order/                       # 业务域：订单
│  ├─ controller/  OrderController.java
│  ├─ service/     OrderService.java          # 接口
│  │  └─ impl/     OrderServiceImpl.java       # 实现
│  ├─ mapper/      OrderMapper.java            # MyBatis Mapper / 或 repository/ 放 JPA Repository
│  ├─ entity/      OrderEntity.java            # 数据库映射对象（表结构）
│  ├─ dto/         OrderCreateDTO.java         # 入参：接收前端/上游
│  ├─ vo/          OrderDetailVO.java          # 出参：返回前端的视图对象
│  ├─ convert/     OrderConvert.java           # Entity <-> DTO/VO 转换（MapStruct）
│  ├─ enums/       OrderStatusEnum.java
│  └─ config/      OrderProperties.java        # 该域的配置绑定
├─ user/                        # 业务域：用户（同构）
├─ common/                      # 跨域基础：Result、异常、BaseEntity、通用工具
│  ├─ exception/   BizException.java, GlobalExceptionHandler.java
│  ├─ response/    Result.java, ResultCode.java
│  └─ base/        BaseEntity.java, PageQuery.java
└─ config/                      # 全局装配：MyBatisPlusConfig, WebMvcConfig, 安全配置
```

多模块（Maven `<modules>`）在规模变大时再拆，典型分：`app-api`（对外接口/DTO/VO）、`app-service`（业务实现）、`app-dao`（Entity/Mapper）、`app-common`（基础设施）。依赖方向 `api → service → dao → common`，禁止反向与环依赖。**中小项目先单模块按域分包，不要过早上多模块。**

- 跨域调用只通过对方 Service 接口，禁止直接 import 对方的 Mapper/Entity。
- `common` 只放真正跨域复用的基础件；不要变成第二个垃圾场。

## 3. 命名规范

| 元素 | 约定 | 示例 |
|---|---|---|
| 包名 | 全小写，业务域单数名词 | `com.example.app.order` |
| 类名 | 大驼峰，名词 | `OrderService` |
| 接口实现 | 接口 `XxxService`，实现 `XxxServiceImpl` | `OrderServiceImpl` |
| 方法名 | 小驼峰动词开头 | `createOrder`、`listByUserId` |
| 常量 | 全大写下划线 | `MAX_RETRY_COUNT` |
| 后缀约定 | `Controller`/`Service`/`ServiceImpl`/`Mapper`/`Repository`/`Entity`/`DTO`/`VO`/`Convert`/`Enum` | 见名知层 |

方法命名建议动词统一：查单条 `getXxx`/`findById`，查列表 `listXxx`，分页 `pageXxx`，存在性 `existsXxx`，计数 `countXxx`，新增 `createXxx`/`saveXxx`，改 `updateXxx`，删 `removeXxx`/`deleteXxx`。

字段类型的硬规则：

```java
// 布尔字段：不要用 isXxx 命名成员变量——Lombok/Jackson 生成的 getter 会踩坑
private Boolean enabled;      // 正：getEnabled()，序列化字段名稳定
// private boolean isEnabled; // 反：基本类型 + is 前缀，getter 变 isEnabled()，
                              //     Jackson 可能序列化成 "enabled" 导致前后端字段名不一致

// 时间字段：统一 LocalDateTime，字段名以 Time 结尾，不要用 java.util.Date
private LocalDateTime createTime;   // 正
private LocalDateTime payTime;      // 正
// private Date create_time;        // 反：Date 已过时、蛇形命名、无时区语义

// 金额：一律 BigDecimal，禁止 double/float（浮点丢精度）
private BigDecimal amount;           // 正
// private double amount;            // 反：0.1+0.2 != 0.3，财务数据必错
// 运算与比较：
BigDecimal total = price.multiply(new BigDecimal(qty))
        .setScale(2, RoundingMode.HALF_UP);   // 显式精度与舍入
if (total.compareTo(BigDecimal.ZERO) > 0) { } // 用 compareTo，禁止 equals 比较金额
```

枚举承载状态，禁止魔法数字散落：

```java
// 正：状态用枚举，带 code 和描述
public enum OrderStatusEnum {
    CREATED(0, "已创建"), PAID(1, "已支付"), CANCELLED(2, "已取消");
    private final int code;
    private final String desc;
    OrderStatusEnum(int code, String desc) { this.code = code; this.desc = desc; }
    public int getCode() { return code; }
    public String getDesc() { return desc; }
}
// 反：if (status == 1) { ... }  // 魔法数字，读者无法知道 1 是什么
```

## 4. DTO / VO / Entity 严格分离

三者职责不同，绝不混用：

- **Entity**：数据库表映射，字段=列。带 `@TableName`/`@Entity`，可能含 `deleted`、`version`、`createBy` 等基础设施字段。
- **DTO**：接收入参（Data Transfer Object），带校验注解，只含前端应当提交的字段。
- **VO**：返回出参（View Object），只含前端需要展示的字段，可含拼装/脱敏后的派生字段。

**为什么 Entity 绝不能直接返给前端**：泄露表结构与内部字段（密码散列、逻辑删除位、内部备注）；字段随表结构改动而破坏 API 契约；懒加载关联在序列化时触发 N+1 或 `LazyInitializationException`；无法按场景裁剪/脱敏。

正例（MapStruct 转换，Entity 不出边界）：

```java
@Mapper(componentModel = "spring")
public interface OrderConvert {
    OrderEntity toEntity(OrderCreateDTO dto);
    OrderDetailVO toVO(OrderEntity entity);
    List<OrderDetailVO> toVOList(List<OrderEntity> list);
}

@RestController
@RequestMapping("/api/orders")
public class OrderController {
    private final OrderService orderService;
    private final OrderConvert orderConvert;
    // ...构造器省略

    @GetMapping("/{id}")
    public Result<OrderDetailVO> detail(@PathVariable Long id) {
        OrderEntity entity = orderService.getById(id);
        return Result.ok(orderConvert.toVO(entity));   // 出口是 VO
    }
}
```

反例（直接把 Entity 甩给前端）：

```java
@GetMapping("/{id}")
public OrderEntity detail(@PathVariable Long id) {
    return orderService.getById(id);   // 反：表结构外泄、字段耦合、脱敏无从谈起
}
```

## 5. Service 层纪律与事务边界

- **业务规则只允许存在于 Service/Domain**。Controller 只做：接收参数、`@Valid` 校验、调用 Service、包装返回。校验、状态流转、金额计算、库存扣减等一律下沉。
- **`@Transactional` 挂在 Service 实现方法上，不挂 Controller**。事务要包住一组必须原子的写操作，边界清晰。
- **自调用事务失效陷阱**：同类内 A 方法直接调用本类带 `@Transactional` 的 B 方法，代理不生效、事务不开启。拆到另一个 Bean，或注入自身代理调用。

反例（业务逻辑塞进 Controller + 事务放错层）：

```java
@PostMapping("/pay")
@Transactional   // 反：事务挂 Controller，代理层级不对，且 Controller 不该管事务
public Result<Void> pay(@RequestBody PayDTO dto) {
    OrderEntity order = orderMapper.selectById(dto.getOrderId());  // 反：Controller 直连 Mapper
    if (order.getStatus() != 1) {                                  // 反：业务判断在 Controller
        return Result.fail("状态不对");
    }
    order.setStatus(2);
    order.setPayTime(LocalDateTime.now());
    orderMapper.updateById(order);
    accountMapper.deduct(dto.getUserId(), order.getAmount());      // 反：多写无原子保证
    return Result.ok();
}
```

重构正例（编排在 Controller，业务与事务在 Service）：

```java
// Controller：只做校验 + 编排
@PostMapping("/pay")
public Result<Void> pay(@Valid @RequestBody PayDTO dto) {
    orderService.pay(dto);
    return Result.ok();
}

// ServiceImpl：业务规则 + 事务边界
@Service
public class OrderServiceImpl implements OrderService {
    private final OrderMapper orderMapper;
    private final AccountService accountService;
    // ...构造器省略

    @Override
    @Transactional(rollbackFor = Exception.class)   // 正：原子写在 Service，任何异常回滚
    public void pay(PayDTO dto) {
        OrderEntity order = orderMapper.selectById(dto.getOrderId());
        if (order == null) {
            throw new BizException(ResultCode.ORDER_NOT_FOUND);
        }
        if (order.getStatus() != OrderStatusEnum.CREATED.getCode()) {
            throw new BizException(ResultCode.ORDER_STATUS_ILLEGAL);   // 业务规则在此
        }
        order.setStatus(OrderStatusEnum.PAID.getCode());
        order.setPayTime(LocalDateTime.now());
        orderMapper.updateById(order);
        accountService.deduct(dto.getUserId(), order.getAmount());     // 同事务内
    }
}
```

## 6. 反屎山硬规则（超标即打回）

- **单类 ≤ 400–500 行**：超了说明职责过多，按内聚拆分（如 `OrderQueryService` / `OrderCommandService`）。
- **单方法 ≤ 50–80 行**：超了抽私有方法，一个方法只讲一件事。
- **圈复杂度 ≤ 10–15**：分支/循环过密就拆，或用策略/状态映射替代长 `if-else`/`switch`。
- **方法参数 ≤ 4**：超了用参数对象（DTO/Query）或 Builder 聚合。
- **嵌套 ≤ 3 层**：用卫语句（guard clause）早返回，把异常/边界前置，主逻辑保持平铺。
- **禁 God Service**：一个 `XxxService` 塞几十个不相关方法、上千行——按用例拆。
- **禁 `Utils`/`CommonUtil` 黑洞**：不要建一个什么都往里塞的静态工具类。工具按主题归类（`MoneyUtils`、`DateUtils`），与业务相关的 helper 放回对应业务域，不进通用工具。
- **禁在 Entity 里堆业务方法**：Entity 是数据载体（充血领域模型是另一套刻意设计，非默认）；默认贫血 + Service 承载业务。
- **DAO/Mapper 不写业务判断**：Mapper 只做取数/存数，`if 状态==x 则...` 属于 Service。

参数对象正例：

```java
// 反：一堆平铺参数，调用点全是位置含义不明的实参
public Page<OrderVO> query(String keyword, Integer status, LocalDateTime start,
                           LocalDateTime end, Long userId, int pageNo, int pageSize) { }

// 正：聚合成查询对象
public Page<OrderVO> query(OrderPageQuery query) { }

@Data
public class OrderPageQuery extends PageQuery {   // PageQuery 提供 pageNo/pageSize
    private String keyword;
    private Integer status;
    private LocalDateTime startTime;
    private LocalDateTime endTime;
    private Long userId;
}
```

卫语句降嵌套正例：

```java
// 反：金字塔嵌套
public void handle(Order o) {
    if (o != null) {
        if (o.getStatus() == 1) {
            if (o.getAmount() != null) {
                // 真正逻辑埋在三层里
            }
        }
    }
}
// 正：早返回，主逻辑平铺
public void handle(Order o) {
    if (o == null) return;
    if (o.getStatus() != 1) return;
    if (o.getAmount() == null) return;
    // 真正逻辑在顶层
}
```

## 7. 异常处理与统一返回

- **统一返回体 `Result<T>`**：所有接口返回 `Result<T>`，含 `code` / `message` / `data`。不返回裸 `Map`、裸实体、裸字符串。
- **统一异常处理 `@RestControllerAdvice`**：业务异常抛 `BizException`，全局处理器兜底转成标准 `Result`；Controller 里不写满屏 try-catch。
- **错误码分级**：成功 `0`/`200`；业务错误用带域前缀的错误码枚举（如 `ORDER_NOT_FOUND`）；系统错误统一 `500` 并记录日志、不把堆栈泄给前端。
- **不吞异常**：禁止 `catch (Exception e) {}` 空吞或只 `e.printStackTrace()`。要么处理、要么带上下文 `log.error` 后重新抛出。
- **参数校验用 `@Valid` + 分组**：新增/更新用不同校验组（`Create.class` / `Update.class`），避免"更新时 id 必填、新增时 id 必空"互相打架。

正例：

```java
@Data
public class OrderCreateDTO {
    @NotNull(message = "商品ID不能为空")
    private Long productId;

    @NotNull @Min(value = 1, message = "数量至少为1")
    private Integer quantity;
}

@RestControllerAdvice
public class GlobalExceptionHandler {
    private static final Logger log = LoggerFactory.getLogger(GlobalExceptionHandler.class);

    @ExceptionHandler(BizException.class)
    public Result<Void> handleBiz(BizException e) {
        return Result.fail(e.getCode(), e.getMessage());   // 业务错误，不记 error 级
    }

    @ExceptionHandler(MethodArgumentNotValidException.class)
    public Result<Void> handleValid(MethodArgumentNotValidException e) {
        String msg = e.getBindingResult().getFieldError().getDefaultMessage();
        return Result.fail(ResultCode.PARAM_INVALID.getCode(), msg);
    }

    @ExceptionHandler(Exception.class)
    public Result<Void> handleSystem(Exception e) {
        log.error("系统异常", e);                            // 记全堆栈到日志
        return Result.fail(ResultCode.SYSTEM_ERROR);        // 只给前端脱敏提示
    }
}
```

反例：

```java
@GetMapping("/{id}")
public Map<String, Object> detail(@PathVariable Long id) {   // 反：裸 Map，无契约
    Map<String, Object> map = new HashMap<>();
    try {
        map.put("data", orderService.getById(id));
    } catch (Exception e) {
        // 反：空吞异常，前端只拿到空 data，问题被掩盖
    }
    return map;
}
```

## 8. 数据访问规范（MyBatis / JPA）

- **禁 `SELECT *`**：显式列出字段，避免多传数据、避免表加列后行为漂移；只需部分字段就用 DTO 投影。
- **必须分页**：列表查询强制分页（MyBatis-Plus `Page` / JPA `Pageable`），禁止无 `LIMIT` 全表捞。
- **消灭 N+1**：需要关联数据时用 `JOIN` 一次查全，或 JPA `@EntityGraph` / `join fetch`；禁止先查列表再循环逐条查详情。
- **禁在循环里查库**：`for` 循环内 `selectById` 是 N+1 的典型；改为 `selectBatchIds(ids)` / `IN` 批量查一次再内存组装。
- **索引意识**：`WHERE`/`ORDER BY` 命中的列要有索引；不在索引列上套函数（`WHERE DATE(create_time)=...` 使索引失效），改用范围查询。
- **DTO 投影**：只取需要的列直接映射到 VO/DTO，减少 IO 与序列化开销。

N+1 反例与批量正例：

```java
// 反：循环内逐条查库，100 个订单打 101 次 SQL
List<OrderEntity> orders = orderMapper.selectList(null);
for (OrderEntity o : orders) {
    UserEntity u = userMapper.selectById(o.getUserId());   // N+1
    o.setUserName(u.getName());
}

// 正：批量取 ID 一次查回，内存组装
List<OrderEntity> orders = orderMapper.selectPage(page, wrapper).getRecords();
Set<Long> userIds = orders.stream().map(OrderEntity::getUserId).collect(toSet());
Map<Long, UserEntity> userMap = userMapper.selectBatchIds(userIds).stream()
        .collect(toMap(UserEntity::getId, u -> u));
orders.forEach(o -> o.setUserName(userMap.get(o.getUserId()).getName()));
```

`SELECT *` 与投影：

```xml
<!-- 反：SELECT *，多取列、表结构变化即受影响 -->
<select id="list" resultType="OrderEntity">SELECT * FROM t_order</select>

<!-- 正：显式列 + 投影到 VO，只取需要的字段 -->
<select id="listVO" resultType="com.example.app.order.vo.OrderListVO">
  SELECT id, order_no, amount, status, create_time
  FROM t_order
  WHERE deleted = 0 AND user_id = #{userId}
  ORDER BY create_time DESC
</select>
```

## 9. 评审清单（写完后逐条勾选）

- [ ] 分层单向：Controller 不含业务、不直连 Mapper；Service 承载业务与事务；Mapper 只取存数据。
- [ ] 按业务域分包，一个需求集中在一个域目录；跨域只经 Service 接口，不深层 import 对方 Entity/Mapper。
- [ ] Service 面向接口 + 构造器注入 `final` 依赖；无字段 `@Autowired`、无依赖具体实现类。
- [ ] DTO（入参 + 校验）/ VO（出参 + 脱敏）/ Entity（表映射）三者分离，Entity 绝不返给前端，用 Convert/MapStruct 转换。
- [ ] 命名合规：后缀约定到位；布尔用包装 `Boolean`、时间用 `LocalDateTime` 且以 Time 结尾、金额用 `BigDecimal` 且显式精度、状态用枚举无魔法数字。
- [ ] `@Transactional` 在 Service 实现且 `rollbackFor = Exception.class`；无自调用导致的事务失效。
- [ ] 反屎山达标：类 ≤ 500 行、方法 ≤ 80 行、参数 ≤ 4、嵌套 ≤ 3（卫语句早返回）；无 God Service、无 Utils 黑洞、无实体堆业务。
- [ ] 统一 `Result<T>` 返回 + `@RestControllerAdvice` 兜底 + 分级错误码；不吞异常、不返裸 Map；`@Valid` 分组校验。
- [ ] 数据访问：无 `SELECT *`、列表必分页、无循环查库/无 N+1、索引列不套函数、需要时用 DTO 投影。

---
**定位**：本文件是 Java/Spring Boot 的 STACK 具体落地；结构分层、依赖方向、反屎山阈值的通用推理见同目录语言无关方法论文件，二者配合注入。

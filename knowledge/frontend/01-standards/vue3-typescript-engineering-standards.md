---
id: vue3-typescript-engineering-standards
title: Vue 3 + TypeScript 前端工程规范（商业级·组件分层·反屎山）
domain: frontend
category: 01-standards
difficulty: advanced
tags: [Vue3, TypeScript, Composition-API, Pinia, 组件分层, 命名规范, 反屎山, frontend]
quality_score: 92
last_updated: 2026-07-12
---

# Vue 3 + TypeScript 前端工程规范（商业级·组件分层·反屎山）

> 这份规范只讲 **Vue 3 + TypeScript** 的落地细节，是"前端架构与分层标准"在 Vue 技术栈上的具体化。分层模型、三类状态分治、API 层隔离、feature-based 分包这些**框架无关的骨架**不在这里重复；本文只回答：SFC 该怎么切、组件怎么命名、逻辑怎么抽 composable、Pinia 边界画在哪、TS 纪律怎么守、屎山怎么防。默认 `<script setup lang="ts">` + Composition API + `strict` TS。

## 0. 一句话原则

**一个 SFC 只做一件事：容器管数据、展示纯渲染；可复用逻辑进 composable，可复用状态进 store；类型显式、props 单向、组件短小。** 超过体量阈值不是"写复杂了"，是"该拆了"。

## 1. 组件分层与目录结构

四类组件，职责边界不许含糊：

| 层 | 位置 | 职责 | 禁止 |
|---|---|---|---|
| 页面 | `features/<x>/views/` | 路由落点、取数、编排、组装区块 | 写复杂展示细节 |
| 业务组件 | `features/<x>/components/` | 绑定某业务域的可复用块（订单卡、审批栏） | 跨域复用、裸取数 |
| 通用 UI | `components/ui/` 或 `shared/ui/` | 无业务的纯展示件（Button/Modal/Table） | import 任何 store/api |
| 布局 | `layouts/` | 页面骨架（侧栏/顶栏/插槽），不含业务 | 取业务数据 |

**按业务域分包（feature-based），不按文件类型分包。** 每个 feature 自带全套分层，对外只经 `index.ts` 暴露：

```
src/features/order/
├─ views/        OrderListView.vue  OrderDetailView.vue   # 页面（容器）
├─ components/   OrderCard.vue  OrderStatusTag.vue         # 业务展示组件
├─ composables/  useOrderList.ts  useOrderActions.ts       # 逻辑
├─ api/          order.api.ts                              # 该域唯一数据出口
├─ store/        useOrderStore.ts                          # 跨组件的域内状态（如需）
├─ types/        order.ts                                  # DTO / 视图模型
└─ index.ts      # 对外 barrel：只导出 views 与公开类型
```

**容器 vs 展示** —— 页面是容器，负责"数据从哪来、事件往哪去"；展示组件只吃 props、抛事件。

正例（展示组件：纯 props → UI，零副作用、零取数）：

```vue
<!-- OrderCard.vue：展示组件 -->
<script setup lang="ts">
import type { Order } from '../types/order'
defineProps<{ order: Order }>()
const emit = defineEmits<{ (e: 'cancel', id: string): void }>()
</script>

<template>
  <article class="order-card">
    <h3>{{ order.title }}</h3>
    <OrderStatusTag :status="order.status" />
    <button @click="emit('cancel', order.id)">取消</button>
  </article>
</template>
```

正例（页面：容器只编排，逻辑委托给 composable）：

```vue
<!-- OrderListView.vue：容器 -->
<script setup lang="ts">
import { useOrderList } from '../composables/useOrderList'
import OrderCard from '../components/OrderCard.vue'
const { orders, isLoading, error, cancel } = useOrderList()
</script>

<template>
  <StateBoundary :loading="isLoading" :error="error" :empty="!orders.length">
    <OrderCard v-for="o in orders" :key="o.id" :order="o" @cancel="cancel" />
  </StateBoundary>
</template>
```

反例（把展示组件写成"什么都干"的巨石：自己取数、自己算、自己渲染，无法复用）：

```vue
<!-- 反例：OrderCard 里裸 fetch + 业务分支，展示组件被污染 -->
<script setup lang="ts">
import axios from 'axios'
const props = defineProps<{ id: string }>()
const order = ref<any>(null)                                  // any + 组件内取数
onMounted(async () => { order.value = (await axios.get('/api/order/' + props.id)).data })
</script>
```

## 2. 命名规范

| 对象 | 规则 | 正例 | 反例 |
|---|---|---|---|
| 组件名 | PascalCase，**多单词**避免与 HTML 冲突 | `UserCard` `OrderStatusTag` | `Card` `Table`（单词） |
| 文件名 | 与组件名一致 | `UserCard.vue` | `user-card.vue` 内含 `UserCard` |
| props 声明 | camelCase | `defineProps<{ maxCount: number }>()` | `max_count` |
| props 模板传入 | kebab-case | `<UserCard :max-count="10" />` | `:maxCount` |
| 事件 | kebab / `update:xxx`（配 v-model） | `@row-click` `update:modelValue` | `@rowClick` |
| composable | `useXxx`，返回响应式 | `useOrderList` | `getOrderData` |
| store | `useXxxStore` | `useOrderStore` | `orderState` |
| 布尔字段 | is/has/can/should 前缀 | `isLoading` `hasError` `canEdit` | `loading`（歧义） |
| 时间字段 | `xxxAt`(时刻) / `xxxMs`(时长) | `createdAt` `timeoutMs` | `time` `date` |
| ref/reactive | 名词，值语义清晰；DOM ref 加 `El` | `count` `formEl` | `data` `temp` `flag` |

命名冲突要点：模板里 `<Order />` 可能被误判为原生元素，业务组件一律用 PascalCase 多单词名。事件用 `update:modelValue` 才能配合 `v-model`。

## 3. Composition API 纪律

**逻辑复用只用 composable，不用 mixin。** mixin 来源不清、命名易撞、类型难追；composable 是显式入参、显式返回、可组合、可单测。

正例（把取数/加载/动作收进一个 composable，页面只解构）：

```ts
// composables/useOrderList.ts
import { ref, onMounted } from 'vue'
import { fetchOrders, cancelOrder } from '../api/order.api'
import type { Order } from '../types/order'

export function useOrderList() {
  const orders = ref<Order[]>([])
  const isLoading = ref(false)
  const error = ref<Error | null>(null)

  async function load() {
    isLoading.value = true
    error.value = null
    try { orders.value = await fetchOrders() }
    catch (e) { error.value = e as Error }
    finally { isLoading.value = false }
  }
  async function cancel(id: string) { await cancelOrder(id); await load() }

  onMounted(load)
  return { orders, isLoading, error, cancel, reload: load }
}
```

**props / emits 必须显式泛型类型**，用 `withDefaults` 给默认值：

```ts
interface Props { size?: 'sm' | 'md' | 'lg'; disabled?: boolean }
const props = withDefaults(defineProps<Props>(), { size: 'md', disabled: false })
const emit = defineEmits<{ (e: 'change', value: string): void }>()
```

**props 单向，禁止在子组件里直接改 props。** 要改就 `emit` 让父级改，或本地拷贝派生：

```ts
// 正例：v-model 双向由父级 emit 承接
const emit = defineEmits<{ (e: 'update:modelValue', v: string): void }>()
const props = defineProps<{ modelValue: string }>()
function onInput(v: string) { emit('update:modelValue', v) }
```

```ts
// 反例：直接改 props —— 破坏单向流，触发警告，行为不可预测
const props = defineProps<{ count: number }>()
function inc() { props.count++ }   // 禁止
```

**巨型 setup 拆分。** 一个 `<script setup>` 里堆了取数 + 表单 + 分页 + 轮询就该按关注点拆成 `useXxx`。判断标准：setup 逻辑超过约 50 行，或出现三个以上互不相干的关注点，立即抽。

## 4. 状态管理边界

三层归属，别把一切塞全局 store：

- **组件本地 `ref`/`reactive`**：只服务当前组件的开关、输入、hover。
- **域内 Pinia store**：跨该 feature 多个组件共享的状态（当前选中、筛选条件、购物车草稿）。
- **服务端数据**：交给取数缓存层管 loading/error/失效，不要手动搬进 store 长期维护。

Pinia 规范：**按域拆 store，异步放 action，getter 纯计算，组件不散落 API 调用**。

正例：

```ts
// store/useOrderStore.ts
import { defineStore } from 'pinia'
import { fetchOrders } from '../api/order.api'
import type { Order } from '../types/order'

export const useOrderStore = defineStore('order', {
  state: () => ({ list: [] as Order[], keyword: '' }),
  getters: {
    // 纯计算，无副作用
    visible: (s): Order[] => s.list.filter(o => o.title.includes(s.keyword)),
  },
  actions: {
    // 异步收在 action，组件不碰 api
    async load() { this.list = await fetchOrders() },
  },
})
```

反例（把 UI 局部态塞全局 + 在组件里散落 fetch + getter 带副作用）：

```ts
// 反例：一个 god store 装下所有页面的 modal 开关、输入框值、请求结果
export const useAppStore = defineStore('app', {
  state: () => ({ orderModalOpen: false, searchInput: '', orders: [], userProfileTab: 0 }),
  getters: { orders: (s) => { fetch('/api/order'); return s.orders } }, // getter 里取数，禁止
})
```

组件里也**禁止裸调 API**——请求一律经 `api/` 层，再由 composable 或 store action 调用：

```ts
// 反例：组件 <script setup> 里直接 axios.get('/api/...')
// 正例：组件 → useOrderList() → order.api.ts → 后端
```

## 5. TypeScript 纪律

- **`strict: true` 全开**（`strictNullChecks` / `noImplicitAny` 生效）。
- **禁 `any`**：不确定用 `unknown` 再收窄，通用逻辑用泛型。
- **API 响应显式建模**：每个接口有 DTO 类型，不让 `any` 从网络边界渗进业务。
- **禁滥用 `as`**：`as` 是绕过类型检查，只在类型断言确有把握（如 `as const`、DOM 精确类型）时用；用它去压 TS 报错就是埋雷。
- **前端请求 URL 与后端路由对齐**：路径集中为常量/枚举，配合类型化 client，改一处即全量对齐。

正例：

```ts
// types/order.ts
export interface OrderDTO { id: string; title: string; status: 'open' | 'paid' | 'closed'; createdAt: string }

// api/order.api.ts —— 路径集中、响应类型化、无 any
const ORDER_API = { list: '/api/orders', cancel: (id: string) => `/api/orders/${id}/cancel` } as const

export async function fetchOrders(): Promise<OrderDTO[]> {
  const res = await http.get<OrderDTO[]>(ORDER_API.list)
  return res.data
}
```

反例：

```ts
// 反例：any 从网络边界扩散 + as 强压 + URL 硬编码散落
async function load() {
  const res: any = await axios.get('/api/oreders')          // any + 拼错的路径无人报错
  list.value = res.data as Order[]                          // as 强转掩盖字段不匹配
}
```

收窄 `unknown` 的正确姿势：

```ts
function isOrder(x: unknown): x is OrderDTO {
  return typeof x === 'object' && x !== null && 'id' in x && 'status' in x
}
```

## 6. 反屎山硬规则（触犯即打回）

- **单 SFC ≤ 300–400 行**：超了按区块拆子组件、按关注点抽 composable。
- **单函数 ≤ 50 行**：分支多就拆纯函数。
- **同一元素禁止 `v-if` + `v-for` 并用**（优先级歧义 + 每轮都判断）：先 `computed` 过滤再 `v-for`。
- **template 嵌套过深要拆**：三层以上条件/循环嵌套抽子组件。
- **禁 god component**：又取数又算逻辑又渲染又管弹窗的巨石组件。
- **禁 `utils/` 黑洞**：和某业务相关的 helper 放进该 feature，别丢进全局 `utils` 大筐。
- **样式走 design token**：颜色/间距/圆角用 CSS 变量或 token，禁硬编码 hex。
- **禁 emoji 当图标/占位**：功能图标一律来自声明的图标库（组件形式），emoji 不是图标。

正例（先 computed 过滤，再 v-for，无 v-if 混用）：

```vue
<script setup lang="ts">
const activeItems = computed(() => items.value.filter(i => i.active))
</script>
<template>
  <ItemRow v-for="i in activeItems" :key="i.id" :item="i" />
</template>
```

反例（同元素 v-if + v-for + 硬编码色 + emoji 图标）：

```vue
<template>
  <!-- v-if 与 v-for 同元素：语义歧义、性能差 -->
  <li v-for="i in items" v-if="i.active" :key="i.id" :style="{ color: '#8b5cf6' }">
    {{ i.name }} 收藏             <!-- 此处直接塞 emoji 字符当收藏图标 + 硬编码紫色 -->
  </li>
</template>
```

正例（token 色 + 图标库组件）：

```vue
<template>
  <li :style="{ color: 'var(--color-text-primary)' }">
    {{ i.name }} <StarIcon class="icon" />   <!-- 来自图标库的组件，非 emoji -->
  </li>
</template>
```

## 7. 评审清单（可勾选）

- [ ] 组件按容器/展示分离：页面取数编排，展示组件只吃 props、抛事件、零副作用。
- [ ] 按业务域 feature-based 分包，每个 feature 自带 views/components/composables/api/store/types，对外只经 `index.ts`。
- [ ] 组件 PascalCase 多单词、文件名一致；props camelCase 声明 / kebab 模板；事件 kebab 或 `update:xxx`；composable `useXxx`、store `useXxxStore`。
- [ ] 逻辑复用用 composable 而非 mixin；props/emits 显式泛型类型；props 单向，无子组件直改 props。
- [ ] 无巨型 setup（超 ~50 行或多关注点已拆 composable）。
- [ ] 状态归属正确：UI 局部态用 ref，域内共享用 Pinia，服务端数据交给缓存层；store 按域拆、异步在 action、getter 纯计算。
- [ ] 组件内无裸 fetch/axios，请求全部经 `api/` 层。
- [ ] `strict` 开启；无 `any`（用 unknown/泛型）；API 响应有 DTO 类型；无滥用 `as`；URL 集中常量、与后端路由对齐。
- [ ] 单 SFC ≤ 300–400 行、单函数 ≤ 50 行；无同元素 v-if+v-for；无 god component、无 utils 黑洞。
- [ ] 颜色/间距走 design token，无硬编码色值；功能图标来自图标库，无 emoji 当图标。

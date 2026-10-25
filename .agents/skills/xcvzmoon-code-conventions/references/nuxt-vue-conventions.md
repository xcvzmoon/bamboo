# Nuxt and Vue conventions

Apply these conventions to Nuxt and Vue files. Preserve dependency order and framework-required hierarchy when it conflicts with the preferred visual order.

## Reactive types

Declare reactive values with explicit types. Apply this to `ref`, `shallowRef`, `reactive`, `computed`, and comparable reactive primitives:

```ts
const count = ref<number>(0);
const selectedItem = shallowRef<Item | null>(null);
const form = reactive<FormState>({ name: "" });
const hasSelection = computed<boolean>(() => selectedItem.value !== null);
```

Do not add an assertion merely to satisfy this rule. Model nullable and initially absent values honestly.

## Script setup order

Use this order in `<script setup>` after imports and type declarations:

1. Constant values
2. `define*` compiler macros and framework definition calls, such as `defineProps`, `defineEmits`, `defineModel`, `defineOptions`, and `definePageMeta`
3. Composable calls
4. Reactive values
5. Computed values
6. Functions
7. Lifecycle hooks

Keep a declaration before its use when moving it would create an initialization, dependency, or readability problem. Place watchers and effects after the values they observe and near the behavior they coordinate.

## Single-file component blocks

Use this block order:

```vue
<script setup lang="ts"></script>

<template></template>

<style lang="css" scoped></style>
```

The style block is optional. Omit an empty style block. Do not place `<template>` before `<script setup>`.

## Template attribute order

Order attributes on native elements and components as follows:

1. `ref`
2. `v-*` directives, including `v-if`, `v-for`, `v-model`, and `v-slot`
3. Dynamic bindings and props written with `:` or `v-bind:`
4. Static attributes and props with values
5. Static boolean attributes and props without values, such as `disabled` or `stacked`
6. Event handlers written with `@` or `v-on:`

Keep coupled directive attributes together when the framework requires it. Let the configured formatter decide wrapping and indentation after ordering.

## Elements and components

- Self-close an empty component when Vue supports it: `<USeparator />` rather than `<USeparator></USeparator>`.
- Self-close empty native elements only when the Vue template compiler and project formatter support that form. Preserve HTML void-element semantics.
- Prefer a component prop over a slot when both produce the same simple content. For example, prefer a Nuxt UI button's `label` prop over a default slot containing only the label text.
- Keep a slot when it carries markup, multiple nodes, scoped data, conditional content, accessibility structure, or behavior that a prop cannot express.

## Blank lines between siblings

Within a template scope containing multiple sibling elements or components, add a blank line between sibling blocks when at least one sibling's markup spans multiple lines. Do not add blank lines when every sibling is a one-line element/component, when the scope has only one child, or when the complete element/component occupies one line.

Keep compact siblings together:

```vue
<div class="border-default border-r px-4 py-3">
  <p class="text-dimmed font-mono text-[0.625rem] uppercase">Tables</p>
  <p class="mt-1 font-mono text-xl font-semibold">{{ tables.length }}</p>
</div>
```

Separate a multiline sibling from the adjacent block:

```vue
<div class="px-4 py-3">
  <p class="text-dimmed font-mono text-[0.625rem] uppercase">State</p>

  <p
    :class="rowsError ? 'text-error' : 'text-success'"
    class="mt-1 font-mono text-xl font-semibold"
  >
    {{ rowsError ? 'ERROR' : 'LIVE' }}
  </p>
</div>
```

When editing a template, normalize the affected scope to these element, attribute, and spacing conventions. Do not turn a focused task into unrelated application-wide formatting.

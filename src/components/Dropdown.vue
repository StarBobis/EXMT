<script setup lang="ts">
/**
 * Fluent-style select. Native <select> popups are drawn by the OS, so their
 * option colors cannot follow the app theme (the dark theme got white-on-white
 * options). This component draws its own listbox in a Teleport and keeps the
 * same keyboard contract: arrows move, Enter selects, Esc closes.
 */
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import type { DropdownOption } from "../lib/dropdown";

const props = withDefaults(
  defineProps<{
    modelValue: string;
    options: DropdownOption[];
    placeholder?: string;
    disabled?: boolean;
    ariaLabel?: string;
    title?: string;
  }>(),
  {
    placeholder: "",
    disabled: false,
    ariaLabel: "",
    title: "",
  },
);

const emit = defineEmits<{
  "update:modelValue": [value: string];
  change: [value: string];
}>();

const open = ref(false);
const activeIndex = ref(-1);
const triggerRef = ref<HTMLButtonElement | null>(null);
const menuRef = ref<HTMLDivElement | null>(null);
const popupStyle = ref<Record<string, string>>({});
const uid = `dd-${Math.random().toString(36).slice(2, 8)}`;
const listId = `${uid}-list`;

const selectedOption = computed(
  () => props.options.find((option) => option.value === props.modelValue) ?? null,
);
const selectedLabel = computed(
  () => selectedOption.value?.label ?? props.placeholder,
);
const isDisabled = computed(
  () => props.disabled || props.options.length === 0,
);

function optionId(index: number) {
  return `${uid}-opt-${index}`;
}

function position() {
  const el = triggerRef.value;
  if (!el) return;
  const rect = el.getBoundingClientRect();
  const width = Math.min(
    Math.max(rect.width, 300),
    Math.max(220, window.innerWidth - 16),
  );
  const spaceBelow = window.innerHeight - rect.bottom - 8;
  const spaceAbove = rect.top - 8;
  const openAbove = spaceBelow < 160 && spaceAbove > spaceBelow;
  const maxHeight = Math.max(
    120,
    Math.min(320, openAbove ? spaceAbove : spaceBelow),
  );
  const left = Math.min(
    Math.max(8, rect.left),
    Math.max(8, window.innerWidth - width - 8),
  );
  popupStyle.value = openAbove
    ? {
        left: `${left}px`,
        bottom: `${window.innerHeight - rect.top + 4}px`,
        width: `${width}px`,
        maxHeight: `${maxHeight}px`,
      }
    : {
        left: `${left}px`,
        top: `${rect.bottom + 4}px`,
        width: `${width}px`,
        maxHeight: `${maxHeight}px`,
      };
}

function addListeners() {
  window.addEventListener("resize", onWindowChange);
  window.addEventListener("scroll", onWindowChange, true);
  document.addEventListener("pointerdown", onDocPointer, true);
}

function removeListeners() {
  window.removeEventListener("resize", onWindowChange);
  window.removeEventListener("scroll", onWindowChange, true);
  document.removeEventListener("pointerdown", onDocPointer, true);
}

function onWindowChange() {
  close();
}

function onDocPointer(event: PointerEvent) {
  const target = event.target as Node | null;
  if (!target) return;
  if (triggerRef.value?.contains(target) || menuRef.value?.contains(target)) {
    return;
  }
  close();
}

function openMenu() {
  if (isDisabled.value || open.value) return;
  const index = props.options.findIndex(
    (option) => option.value === props.modelValue,
  );
  activeIndex.value = index >= 0 ? index : 0;
  position();
  open.value = true;
  addListeners();
}

function close(refocus = false) {
  if (!open.value) return;
  open.value = false;
  removeListeners();
  if (refocus) {
    void nextTick(() => triggerRef.value?.focus());
  }
}

function toggle() {
  if (open.value) close(true);
  else openMenu();
}

function move(delta: number) {
  const count = props.options.length;
  if (!count) return;
  activeIndex.value =
    (activeIndex.value + delta + count) % count;
  void nextTick(() => {
    document
      .getElementById(optionId(activeIndex.value))
      ?.scrollIntoView({ block: "nearest" });
  });
}

function choose(index: number) {
  const option = props.options[index];
  if (!option) return;
  emit("update:modelValue", option.value);
  emit("change", option.value);
  close(true);
}

function onKeydown(event: KeyboardEvent) {
  if (isDisabled.value) return;
  switch (event.key) {
    case "ArrowDown":
    case "ArrowUp":
      event.preventDefault();
      if (!open.value) {
        openMenu();
        return;
      }
      move(event.key === "ArrowDown" ? 1 : -1);
      return;
    case "Enter":
    case " ":
      event.preventDefault();
      if (!open.value) {
        openMenu();
        return;
      }
      choose(activeIndex.value);
      return;
    case "Escape":
      if (open.value) {
        event.preventDefault();
        close(true);
      }
      return;
    case "Tab":
      if (open.value) close();
      return;
    case "Home":
      if (open.value) {
        event.preventDefault();
        activeIndex.value = 0;
      }
      return;
    case "End":
      if (open.value) {
        event.preventDefault();
        activeIndex.value = props.options.length - 1;
      }
      return;
    default:
  }
}

watch(
  () => props.modelValue,
  () => {
    if (!open.value) return;
    const index = props.options.findIndex(
      (option) => option.value === props.modelValue,
    );
    activeIndex.value = index >= 0 ? index : 0;
  },
);

watch(
  () => props.options.length,
  (length) => {
    if (activeIndex.value >= length) {
      activeIndex.value = Math.max(0, length - 1);
    }
  },
);

onBeforeUnmount(() => {
  removeListeners();
});
</script>

<template>
  <div class="dropdown" :class="{ 'dropdown--open': open }">
    <button
      ref="triggerRef"
      class="dropdown__trigger"
      type="button"
      role="combobox"
      aria-haspopup="listbox"
      :aria-label="ariaLabel"
      :aria-expanded="open"
      :aria-controls="listId"
      :aria-activedescendant="open ? optionId(activeIndex) : undefined"
      :title="title || selectedLabel"
      :disabled="isDisabled"
      @click="toggle"
      @keydown="onKeydown"
    >
      <span
        class="dropdown__value"
        :class="{ 'dropdown__value--placeholder': !selectedOption }"
      >
        {{ selectedLabel }}
      </span>
      <span class="dropdown__chevron glyph" aria-hidden="true">&#xE70D;</span>
    </button>

    <Teleport to="body">
      <div
        v-if="open"
        :id="listId"
        ref="menuRef"
        class="dropdown__menu"
        role="listbox"
        :aria-label="ariaLabel"
        :style="popupStyle"
        tabindex="-1"
        @pointerdown.stop
      >
        <button
          v-for="(option, index) in options"
          :id="optionId(index)"
          :key="option.value"
          class="dropdown__option"
          :class="{
            'dropdown__option--active': index === activeIndex,
            'dropdown__option--selected': option.value === modelValue,
          }"
          type="button"
          role="option"
          :aria-selected="option.value === modelValue"
          @pointermove="activeIndex = index"
          @click="choose(index)"
        >
          <span class="dropdown__check glyph" aria-hidden="true">
            {{ option.value === modelValue ? "\u2713" : "" }}
          </span>
          <span class="dropdown__option-label">{{ option.label }}</span>
        </button>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.dropdown {
  position: relative;
  width: 100%;
  min-width: 0;
}

.dropdown__trigger {
  width: 100%;
  height: 30px;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 8px 0 10px;
  border: 1px solid var(--stroke-strong);
  border-radius: var(--radius-sm);
  background: var(--card-bg);
  color: var(--text-1);
  font: inherit;
  font-size: 11.5px;
  line-height: 1;
  text-align: left;
  cursor: pointer;
  transition:
    background-color var(--dur-fast) var(--ease),
    border-color var(--dur-fast) var(--ease);
}

.dropdown__trigger:hover:not(:disabled) {
  background: var(--fill-subtle-hover);
}

.dropdown--open .dropdown__trigger {
  border-color: var(--accent);
}

.dropdown__trigger:disabled {
  opacity: 0.5;
  cursor: default;
}

.dropdown__trigger:focus,
.dropdown__trigger:focus-visible {
  outline: none;
}

.dropdown__value {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dropdown__value--placeholder {
  color: var(--text-3);
}

.dropdown__chevron {
  flex: none;
  font-size: 10px;
  color: var(--text-2);
  transition: transform var(--dur-fast) var(--ease);
}

.dropdown--open .dropdown__chevron {
  transform: rotate(180deg);
}

.dropdown__menu {
  position: fixed;
  z-index: 1000;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 4px;
  border: 1px solid var(--stroke-strong);
  border-radius: var(--radius-lg);
  background: var(--bg-elevated);
  box-shadow: var(--shadow-flyout);
  overflow-y: auto;
  overscroll-behavior: contain;
}

.dropdown__option {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-height: 30px;
  padding: 4px 8px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-1);
  font: inherit;
  font-size: 11.5px;
  line-height: 16px;
  text-align: left;
  cursor: pointer;
}

.dropdown__option:hover,
.dropdown__option--active {
  background: var(--fill-subtle-hover);
}

.dropdown__option--selected {
  background: var(--accent-soft);
}

.dropdown__option--selected.dropdown__option--active {
  background: var(--accent-soft-strong);
}

.dropdown__check {
  flex: none;
  width: 14px;
  color: var(--accent);
  font-size: 11px;
  text-align: center;
}

.dropdown__option-label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dropdown__option:focus,
.dropdown__option:focus-visible {
  outline: none;
}
</style>

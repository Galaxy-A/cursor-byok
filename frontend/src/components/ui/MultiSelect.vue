
<script setup>
import { autoUpdate, computePosition, flip, offset, shift, size } from "@floating-ui/dom";
import { computed, onBeforeUnmount, nextTick, ref, watch, watchPostEffect } from "vue";

const props = defineProps({
  modelValue: {
    type: Array,
    default: () => [],
  },
  options: {
    type: Array,
    default: () => [],
  },
  placeholder: { type: String, default: "请选择" },
  disabled: { type: Boolean, default: false },
  ariaLabel: { type: String, default: "" },
  summaryFormatter: { type: Function, default: null },
});

const emit = defineEmits(["update:modelValue", "change"]);

const rootRef = ref(null);
const buttonRef = ref(null);
const menuRef = ref(null);
const selectAllRef = ref(null);
const optionRefs = ref([]);
const isOpen = ref(false);
const menuStyle = ref({});

// -1 表示"全选"按钮，0..n-1 表示选项，共同组成一个可循环的键盘焦点环
const activeIndex = ref(-1);

const normalizedOptions = computed(() => props.options.map((option) => {
  if (typeof option === "string") {
    return { label: option, value: option };
  }
  return {
    label: option?.label ?? option?.value ?? "",
    value: option?.value ?? "",
    icon: option?.icon ?? "",
  };
}));

const selectedValues = computed(() => new Set(props.modelValue ?? []));
const allSelected = computed(() =>
  normalizedOptions.value.length > 0
  && normalizedOptions.value.every((option) => selectedValues.value.has(option.value)),
);
const summaryLabel = computed(() => {
  const count = selectedValues.value.size;
  if (count === 0) {
    return props.placeholder;
  }
  if (props.summaryFormatter) {
    return props.summaryFormatter(count, normalizedOptions.value.length);
  }
  return `已选择 ${count} 项`;
});

function emitSelection(values) {
  emit("update:modelValue", values);
  emit("change", values);
}

function toggleOption(option) {
  const next = normalizedOptions.value
    .filter((item) => (item.value === option.value
      ? !selectedValues.value.has(item.value)
      : selectedValues.value.has(item.value)))
    .map((item) => item.value);
  emitSelection(next);
}

function toggleSelectAll() {
  if (allSelected.value) {
    emitSelection([]);
    return;
  }
  emitSelection(normalizedOptions.value.map((option) => option.value));
}

function setOptionRef(el, index) {
  if (el) {
    optionRefs.value[index] = el;
    return;
  }
  delete optionRefs.value[index];
}

function focusActiveOption() {
  nextTick(() => {
    if (activeIndex.value < 0) {
      selectAllRef.value?.focus();
      return;
    }
    optionRefs.value[activeIndex.value]?.focus();
  });
}

function moveActiveIndex(step) {
  if (!isOpen.value) {
    openMenu();
    return;
  }
  const total = normalizedOptions.value.length;
  if (total === 0) {
    return;
  }
  // 焦点环长度为 total + 1（含全选），内部用 0..total 表示，再映射回 -1..total-1
  const ringSize = total + 1;
  const current = activeIndex.value + 1;
  activeIndex.value = ((current + step + ringSize) % ringSize) - 1;
  focusActiveOption();
}

function openMenu() {
  if (props.disabled || isOpen.value) {
    return;
  }
  isOpen.value = true;
  const firstSelected = normalizedOptions.value.findIndex((option) => selectedValues.value.has(option.value));
  activeIndex.value = firstSelected;
  nextTick(() => {
    updatePosition();
    focusActiveOption();
  });
}

function closeMenu({ restoreFocus = false } = {}) {
  if (!isOpen.value) {
    return;
  }
  isOpen.value = false;
  activeIndex.value = -1;
  optionRefs.value = [];
  menuStyle.value = {};
  if (restoreFocus) {
    nextTick(() => buttonRef.value?.focus());
  }
}

function toggleMenu() {
  if (isOpen.value) {
    closeMenu();
    return;
  }
  openMenu();
}

function handleButtonKeydown(event) {
  if (props.disabled) {
    return;
  }
  switch (event.key) {
    case "ArrowDown":
      event.preventDefault();
      moveActiveIndex(1);
      break;
    case "ArrowUp":
      event.preventDefault();
      moveActiveIndex(-1);
      break;
    case "Enter":
    case " ":
      event.preventDefault();
      toggleMenu();
      break;
    case "Escape":
      if (isOpen.value) {
        event.preventDefault();
        closeMenu();
      }
      break;
    default:
      break;
  }
}

function handleOptionKeydown(event, option, index) {
  switch (event.key) {
    case "ArrowDown":
      event.preventDefault();
      activeIndex.value = index;
      moveActiveIndex(1);
      break;
    case "ArrowUp":
      event.preventDefault();
      activeIndex.value = index;
      moveActiveIndex(-1);
      break;
    case "Enter":
    case " ":
      event.preventDefault();
      if (option) {
        toggleOption(option);
        break;
      }
      toggleSelectAll();
      break;
    case "Escape":
      event.preventDefault();
      closeMenu({ restoreFocus: true });
      break;
    case "Tab":
      closeMenu();
      break;
    default:
      break;
  }
}

function handlePointerDown(event) {
  if (rootRef.value?.contains(event.target) || menuRef.value?.contains(event.target)) {
    return;
  }
  closeMenu();
}

function updatePosition() {
  if (!buttonRef.value || !menuRef.value) {
    return;
  }
  computePosition(buttonRef.value, menuRef.value, {
    placement: "bottom-start",
    middleware: [
      offset(6),
      flip({ padding: 12 }),
      shift({ padding: 12 }),
      size({
        apply({ rects, elements, availableHeight }) {
          Object.assign(elements.floating.style, {
            minWidth: `${rects.reference.width}px`,
            maxHeight: `${Math.max(availableHeight, 200)}px`,
          });
        },
        padding: 12,
      }),
    ],
  }).then(({ x, y }) => {
    menuStyle.value = {
      left: `${x}px`,
      top: `${y}px`,
    };
  });
}

watchPostEffect((cleanup) => {
  if (!isOpen.value || !buttonRef.value || !menuRef.value) {
    return;
  }
  const stopAutoUpdate = autoUpdate(buttonRef.value, menuRef.value, updatePosition);
  cleanup(() => {
    stopAutoUpdate();
  });
});

watch(isOpen, (open) => {
  if (open) {
    document.addEventListener("pointerdown", handlePointerDown);
    return;
  }
  document.removeEventListener("pointerdown", handlePointerDown);
});

watch(() => props.disabled, (disabled) => {
  if (disabled) {
    closeMenu();
  }
});

onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", handlePointerDown);
});
</script>

<template>
  <div ref="rootRef" class="relative">
    <button
      ref="buttonRef"
      type="button"
      :disabled="disabled"
      class="flex h-9 w-full items-center justify-between gap-2 rounded-[6px] border border-[#3f3f3f] bg-[#232323] px-3 text-left text-sm text-[#e5e5e5] outline-none transition-colors focus:border-[#10AD5D] disabled:cursor-not-allowed disabled:opacity-60"
      :aria-expanded="isOpen"
      :aria-label="ariaLabel || undefined"
      aria-haspopup="listbox"
      @click="toggleMenu"
      @keydown="handleButtonKeydown"
    >
      <span class="flex min-w-0 flex-1 items-center gap-2" :class="selectedValues.size ? 'text-[#e5e5e5]' : 'text-[#7b7b7b]'">
        <span class="truncate">{{ summaryLabel }}</span>
      </span>
      <span
        class="pointer-events-none center-row text-[#8f8f8f] transition-transform duration-200"
        :class="isOpen ? 'rotate-180' : ''"
      >
        <span class="icon-[mdi--chevron-down] text-[18px]"></span>
      </span>
    </button>
  </div>

  <Teleport to="body">
    <Transition
      enter-active-class="transition duration-150 ease-out"
      enter-from-class="translate-y-1 opacity-0"
      enter-to-class="translate-y-0 opacity-100"
      leave-active-class="transition duration-100 ease-in"
      leave-from-class="translate-y-0 opacity-100"
      leave-to-class="translate-y-1 opacity-0"
    >
      <div
        v-if="isOpen"
        ref="menuRef"
        class="fixed z-[999] flex flex-col overflow-hidden rounded-[8px] border border-[#3f3f3f] bg-[#232323] p-1 shadow-[0_16px_30px_-12px_rgba(0,0,0,0.7)]"
        :style="menuStyle"
      >
        <button
          ref="selectAllRef"
          type="button"
          class="flex w-full items-center gap-2 rounded-[6px] px-3 py-2 text-left text-sm text-[#d4d4d4] outline-none transition-colors hover:bg-[#303030]"
          :class="activeIndex === -1 ? 'bg-[#303030]' : ''"
          @click="toggleSelectAll"
          @mouseenter="activeIndex = -1"
          @keydown="handleOptionKeydown($event, null, -1)"
        >
          <span :class="[allSelected ? 'icon-[mdi--checkbox-marked]' : 'icon-[mdi--checkbox-blank-outline]', 'text-[16px] shrink-0']"></span>
          <span class="truncate">{{ allSelected ? "取消全选" : "全选" }}</span>
        </button>

        <ul role="listbox" aria-multiselectable="true" class="overflow-y-auto py-1">
          <li v-for="(option, index) in normalizedOptions" :key="option.value">
            <button
              :ref="(el) => setOptionRef(el, index)"
              type="button"
              role="option"
              class="flex w-full items-center gap-2 rounded-[6px] px-3 py-2 text-left text-sm outline-none transition-colors"
              :class="[
                selectedValues.has(option.value)
                  ? 'bg-[#10AD5D]/15 text-[#10d06f]'
                  : 'text-[#e5e5e5] hover:bg-[#303030]',
                activeIndex === index ? 'bg-[#303030]' : '',
              ]"
              :aria-selected="selectedValues.has(option.value)"
              tabindex="0"
              @click="toggleOption(option)"
              @mouseenter="activeIndex = index"
              @keydown="handleOptionKeydown($event, option, index)"
            >
              <span
                :class="[
                  selectedValues.has(option.value) ? 'icon-[mdi--checkbox-marked]' : 'icon-[mdi--checkbox-blank-outline]',
                  'text-[16px] shrink-0',
                ]"
              ></span>
              <span v-if="option.icon" :class="[option.icon, 'text-[16px] shrink-0']" aria-hidden="true"></span>
              <span class="truncate">{{ option.label }}</span>
            </button>
          </li>
        </ul>
      </div>
    </Transition>
  </Teleport>
</template>

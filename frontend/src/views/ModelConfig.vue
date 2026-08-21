<script setup>
import Button from "@/components/ui/Button.vue";
import Card from "@/components/ui/Card.vue";
import ModelAdapterTestCard from "@/components/ModelAdapterTestCard.vue";
import { useConfigTransfer } from "@/composables/useConfigTransfer";
import { useMessage } from "@/composables/useMessage";
import { showModal } from "@/composables/useModal";
import Sortable from "sortablejs";
import {
  appState,
  createEmptyModelAdapter,
  deleteModelAdapterAt,
  duplicateModelAdapterAt,
  getModelAdapterTestResultByID,
  openModelEditorWindow,
  reloadUserConfig,
  runModelAdapterTest,
  saveModelAdapterOrder,
  startModelAdapterTest,
  toUserError,
} from "@/state/appState";
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";

const BATCH_TEST_CONCURRENCY = 10;
const message = useMessage();

const typeTabs = [
  { label: "全部", value: "all", icon: "icon-[mdi--view-grid-outline]" },
  { label: "OpenAI", value: "openai", icon: "icon-[bxl--openai]" },
  { label: "Anthropic", value: "anthropic", icon: "icon-[logos--claude-icon]" },
];

const activeType = ref("all");
const batchTesting = ref(false);
const batchStopping = ref(false);
const batchTotal = ref(0);
const batchCompleted = ref(0);
const modelGrid = ref(null);
const sortSaving = ref(false);
const batchActiveCalls = new Set();
let batchStopRequested = false;
let sortable = null;

const filteredAdapters = computed(() => (
  activeType.value === "all"
    ? appState.modelAdapters
    : appState.modelAdapters.filter((adapter) => adapter.type === activeType.value)
));
const filteredAdapterOrderKey = computed(() => filteredAdapters.value.map((adapter) => adapter.id).join("\n"));
const batchButtonText = computed(() => {
  if (batchStopping.value) {
    return "停止中...";
  }
  if (!batchTesting.value) {
    return "测试全部";
  }
  return `停止测试 ${batchCompleted.value}/${batchTotal.value}`;
});

watch(
  () => appState.modelAdapters,
  (adapters) => {
    if (activeType.value === "all" || adapters.some((adapter) => adapter.type === activeType.value)) {
      return;
    }
    activeType.value = "all";
  },
  { deep: true, immediate: true },
);

async function showActionError(title, error) {
  await showModal({
    title,
    content: String(error || "服务错误").trim() || "服务错误",
  });
}

function maskSecret(value) {
  const text = String(value || "").trim();
  if (!text) {
    return "-";
  }
  if (text.length <= 8) {
    return `${"*".repeat(Math.max(text.length - 2, 0))}${text.slice(-2)}`;
  }
  return `${text.slice(0, 4)}****${text.slice(-4)}`;
}

function typeLabel(type) {
  return type === "anthropic" ? "Anthropic" : "OpenAI";
}

function formatHost(value) {
  const text = String(value || "").trim();
  if (!text) {
    return "-";
  }
  try {
    const parsed = new URL(text);
    return parsed.host || text;
  } catch {
    return text.replace(/^https?:\/\//, "");
  }
}

async function openEditor(index = -1) {
  const adapter = index >= 0
    ? appState.modelAdapters[index]
    : {
        ...createEmptyModelAdapter(),
        type: activeType.value === "anthropic" ? "anthropic" : "openai",
      };
  try {
    await openModelEditorWindow(index, adapter);
  } catch (error) {
    await showActionError("打开失败", toUserError(error));
  }
}

const {
  configTransferBusy,
  handleExportConfig,
  handleImportConfig,
} = useConfigTransfer({
  showSuccess: (content) => message.success(content),
  showActionError,
});

function destroySortable() {
  if (sortable) {
    sortable.destroy();
    sortable = null;
  }
}

function syncSortable() {
  const element = modelGrid.value;
  if (!element) {
    destroySortable();
    return;
  }
  if (!sortable || sortable.el !== element) {
    destroySortable();
    sortable = Sortable.create(element, {
      animation: 160,
      dataIdAttr: "data-model-id",
      draggable: ".model-sort-item",
      handle: ".model-sort-handle",
      ghostClass: "opacity-40",
      chosenClass: "!border-[#10AD5D]",
      onEnd: (event) => {
        void handleModelSort(event);
      },
    });
  }
  sortable.option("disabled", sortSaving.value || appState.configSaving || batchTesting.value);
  sortable.sort(filteredAdapters.value.map((adapter) => adapter.id), false);
}

async function handleModelSort(event) {
  const oldIndex = event.oldDraggableIndex ?? event.oldIndex;
  const newIndex = event.newDraggableIndex ?? event.newIndex;
  if (!Number.isInteger(oldIndex) || !Number.isInteger(newIndex) || oldIndex === newIndex) {
    syncSortable();
    return;
  }

  const reordered = filteredAdapters.value.slice();
  const [moved] = reordered.splice(oldIndex, 1);
  if (!moved || newIndex < 0 || newIndex > reordered.length) {
    syncSortable();
    return;
  }
  reordered.splice(newIndex, 0, moved);

  const previousAdapters = appState.modelAdapters.slice();
  let nextAdapters = reordered;
  if (activeType.value !== "all") {
    let typeIndex = 0;
    nextAdapters = previousAdapters.map((adapter) => {
      if (adapter.type !== activeType.value) {
        return adapter;
      }
      const nextAdapter = reordered[typeIndex];
      typeIndex += 1;
      return nextAdapter;
    });
  }
  nextAdapters = nextAdapters.map((adapter, index) => ({ ...adapter, sort: index + 1 }));

  sortSaving.value = true;
  appState.modelAdapters = nextAdapters;
  try {
    const result = await saveModelAdapterOrder(nextAdapters.map((adapter) => adapter.id));
    if (!result.ok) {
      appState.modelAdapters = previousAdapters;
      await showActionError("排序失败", result.error);
    }
  } catch (error) {
    appState.modelAdapters = previousAdapters;
    await showActionError("排序失败", toUserError(error));
  } finally {
    sortSaving.value = false;
    await nextTick();
    syncSortable();
  }
}

watch(
  () => [modelGrid.value, activeType.value, filteredAdapterOrderKey.value, appState.configSaving, batchTesting.value],
  () => { void nextTick().then(syncSortable); },
  { flush: "post" },
);

async function handleDeleteModelAdapter(index) {
  const target = appState.modelAdapters[index];
  if (!target) {
    await showActionError("删除失败", "模型配置不存在，无法删除");
    return;
  }
  const result = await deleteModelAdapterAt(index);
  if (!result.ok) {
    await showActionError("删除失败", result.error);
  }
}

async function handleDuplicateModelAdapter(index) {
  const target = appState.modelAdapters[index];
  if (!target) {
    await showActionError("复制失败", "模型配置不存在，无法复制");
    return;
  }
  const result = await duplicateModelAdapterAt(index);
  if (!result.ok) {
    await showActionError("复制失败", result.error);
  }
}

function getAdapterTestResult(adapter) {
  return getModelAdapterTestResultByID(adapter?.id);
}

function isAdapterTesting(adapter) {
  return getAdapterTestResult(adapter)?.status === "running";
}

async function handleTestModelAdapter(adapter) {
  try {
    await runModelAdapterTest(adapter);
  } catch (_error) {
    // 失败结果会通过事件同步到界面，这里不再额外弹窗打断用户。
  }
}

function isCancelError(error) {
  return String(error?.name || "").trim() === "CancelError";
}

async function stopBatchTesting() {
  if (!batchTesting.value || batchStopping.value) {
    return;
  }
  batchStopRequested = true;
  batchStopping.value = true;
  const activeCalls = Array.from(batchActiveCalls);
  await Promise.allSettled(
    activeCalls.map((call) => (typeof call?.cancel === "function" ? call.cancel("batch-stop") : undefined)),
  );
}

async function handleTestAllModelAdapters() {
  if (batchTesting.value) {
    await stopBatchTesting();
    return;
  }
  const adapters = filteredAdapters.value.slice();
  if (adapters.length === 0) {
    return;
  }
  batchStopRequested = false;
  batchTesting.value = true;
  batchStopping.value = false;
  batchTotal.value = adapters.length;
  batchCompleted.value = 0;
  let nextIndex = 0;
  try {
    const workers = Array.from({ length: Math.min(BATCH_TEST_CONCURRENCY, adapters.length) }, async () => {
      while (!batchStopRequested) {
        const currentIndex = nextIndex;
        nextIndex += 1;
        if (currentIndex >= adapters.length) {
          return;
        }
        const adapter = adapters[currentIndex];
        const call = startModelAdapterTest(adapter);
        batchActiveCalls.add(call);
        try {
          await call;
        } catch (error) {
          if (!isCancelError(error) && !batchStopRequested) {
            // 单个失败结果由卡片自行展示，这里继续后续测试。
          }
        } finally {
          batchActiveCalls.delete(call);
          batchCompleted.value += 1;
        }
      }
    });
    await Promise.allSettled(workers);
  } finally {
    batchActiveCalls.clear();
    batchStopRequested = false;
    batchTesting.value = false;
    batchStopping.value = false;
  }
}

onMounted(async () => {
  await reloadUserConfig({ modelAdaptersOnly: true }).catch(() => { });
  await nextTick();
  syncSortable();
});

onBeforeUnmount(() => {
  void stopBatchTesting();
  destroySortable();
});
</script>

<template>
  <div class="flex h-full min-h-0 flex-col p-4 pt-0 text-[#e5e5e5] overflow-hidden">
    <div class="shrink-0 pb-4">
      <div class="flex items-center justify-between gap-4">
        <div class="center-row gap-2">
          <button
            v-for="tab in typeTabs"
            :key="tab.value"
            type="button"
            class="center-row gap-2 rounded-[8px] border px-3 py-2 text-sm transition-colors duration-150"
            :class="activeType === tab.value
              ? 'border-[#1ca35a] bg-[#123322] text-white'
              : 'border-[#343434] bg-[#252525] text-[#a3a3a3] hover:border-[#4a4a4a] hover:text-[#e5e5e5]'"
            @click="activeType = tab.value"
          >
            <span :class="[tab.icon, 'text-[16px]']"></span>
            <span>{{ tab.label }}</span>
          </button>
        </div>
        <div class="center-row gap-2">
          <Button
            variant="default"
            :disabled="sortSaving || appState.configSaving || batchTesting || configTransferBusy || appState.serviceRunning || appState.backendRunning || appState.proxyRunning"
            @click="handleImportConfig"
          >
            导入配置
          </Button>
          <Button
            variant="default"
            :disabled="sortSaving || appState.configSaving || batchTesting || configTransferBusy"
            @click="handleExportConfig"
          >
            导出配置
          </Button>
          <Button
            variant="default"
            :disabled="sortSaving || appState.configSaving || configTransferBusy || (!batchTesting && filteredAdapters.length === 0)"
            @click="handleTestAllModelAdapters"
          >
            {{ batchButtonText }}
          </Button>
          <Button variant="primary" :disabled="sortSaving || appState.configSaving || batchTesting || configTransferBusy" @click="openEditor()">新增模型</Button>
        </div>
      </div>
    </div>

    <div class="min-h-0 flex-1">
      <div v-if="filteredAdapters.length === 0"
        class="flex h-full min-h-[220px] items-center justify-center rounded-[8px] border border-dashed border-[#3a3a3a] bg-[#232323] px-4 text-sm text-[#a3a3a3]">
        {{ activeType === "all" ? "当前还没有配置任何模型。" : `当前还没有配置任何 ${typeLabel(activeType)} 模型。` }}
      </div>

      <div v-else class="h-full min-h-0 overflow-y-auto pr-1">
        <div ref="modelGrid" class="grid gap-3 pb-1 [grid-template-columns:repeat(auto-fill,minmax(250px,1fr))]">
          <Card
            v-for="(adapter, index) in filteredAdapters"
            :key="adapter.id || `${adapter.baseURL}-${adapter.modelID}-${index}`"
            class="model-sort-item group relative"
            :data-model-id="adapter.id"
          >
            <button
              type="button"
              class="model-sort-handle absolute left-2 top-2 z-10 flex h-[30px] w-[30px] touch-none cursor-grab items-center justify-center rounded-[6px] border border-[#454545] bg-[#303030] text-[#d4d4d4] opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 disabled:cursor-not-allowed disabled:opacity-30"
              :disabled="sortSaving || appState.configSaving || batchTesting"
              aria-label="拖拽排序"
              title="拖拽排序"
              @click.stop
            >
              <span class="icon-[icon-park-outline--drag] text-[18px]"></span>
            </button>
            <div class="flex h-full min-h-[154px] flex-col justify-between gap-3">
              <div class="flex flex-col gap-2.5">
                <div class="flex items-start justify-between gap-3">
                  <div class="min-w-0 flex-1">
                    <div class="truncate text-base font-medium text-white">{{ adapter.displayName }}</div>
                    <div class="mt-1 truncate text-sm text-[#8f8f8f]">{{ adapter.modelID }}</div>
                    <div v-if="adapter.type === 'openai'" class="mt-0.5 truncate text-xs text-[#737373]">
                      {{ adapter.openAIEndpoint || "/v1/responses" }}
                    </div>
                  </div>
                  <span
                    class="center-row shrink-0 gap-1 rounded-[999px] border border-[#3f3f3f] px-[7px] py-[4px] text-[11px] font-medium text-[#cfcfcf]"
                  >
                    <span class="icon-[bxl--openai] text-[14px] !text-white" v-if="adapter.type === 'openai'"></span>
                    <span class="icon-[logos--claude-icon] text-[14px]" v-else></span>
                    <span>{{ typeLabel(adapter.type) }}</span>
                  </span>
                </div>

                <div class="grid grid-cols-2 gap-2 text-sm text-[#a3a3a3]">
                  <div class="rounded-[8px] bg-[#232323] px-3 py-2">
                    <div class="text-[11px] uppercase tracking-[0.08em] text-[#666]">Host</div>
                    <div class="mt-1 truncate text-[#d4d4d4]" :title="adapter.baseURL">{{ formatHost(adapter.baseURL) }}</div>
                  </div>
                  <div class="rounded-[8px] bg-[#232323] px-3 py-2">
                    <div class="text-[11px] uppercase tracking-[0.08em] text-[#666]">API Key</div>
                    <div class="mt-1 truncate text-[#d4d4d4]">{{ maskSecret(adapter.apiKey) }}</div>
                  </div>
                </div>

                <ModelAdapterTestCard
                  compact
                  title="测试"
                  empty-text="未测试"
                  :result="getAdapterTestResult(adapter)"
                />
              </div>

              <div class="center-row flex-wrap justify-end gap-2 border-t border-[#343434] pt-3">
                <Button
                  variant="default"
                  :disabled="sortSaving || appState.configSaving || batchTesting || isAdapterTesting(adapter)"
                  @click="handleTestModelAdapter(adapter)"
                >
                  {{ isAdapterTesting(adapter) ? "测试中..." : "测试" }}
                </Button>
                <Button variant="default" :disabled="sortSaving || appState.configSaving" @click="openEditor(appState.modelAdapters.indexOf(adapter))">编辑</Button>
                <Button variant="default" :disabled="sortSaving || appState.configSaving" @click="handleDuplicateModelAdapter(appState.modelAdapters.indexOf(adapter))">复制</Button>
                <Button variant="text" :disabled="sortSaving || appState.configSaving"
                  @click="handleDeleteModelAdapter(appState.modelAdapters.indexOf(adapter))">删除</Button>
              </div>
            </div>
          </Card>
        </div>
      </div>
    </div>
  </div>
</template>

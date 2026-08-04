<script setup>
import Button from "@/components/ui/Button.vue";
import Card from "@/components/ui/Card.vue";
import { showModal } from "@/composables/useModal";
import {
  disconnectCursorAccount,
  getCursorAccountStatus,
  startCursorAccountLogin,
} from "@/services/clientApi";
import { toUserError } from "@/state/appState";
import { computed, onMounted, onUnmounted, ref } from "vue";

const status = ref({ state: "signed_out", authId: "", email: "", error: "" });
const busy = ref(false);
let timer = null;

const signedIn = computed(() => status.value.state === "signed_in");
const waiting = computed(() => status.value.state === "waiting");
const stateText = computed(() => {
  if (signedIn.value) return "已登录";
  if (waiting.value) return "等待浏览器登录";
  return "未连接";
});

async function showError(title, error) {
  await showModal({ title, content: String(error || "服务错误").trim() || "服务错误" });
}

async function refresh() {
  status.value = await getCursorAccountStatus();
}

async function login() {
  busy.value = true;
  try {
    status.value = await startCursorAccountLogin();
  } catch (error) {
    await showError("登录失败", toUserError(error));
    await refresh().catch(() => {});
  } finally {
    busy.value = false;
  }
}

async function disconnect() {
  const confirmed = await showModal({
    title: "退出登录",
    content: "只会退出本应用中的 Cursor 控制面账号，不会退出 Cursor 客户端。是否继续？",
    confirmText: "退出登录",
    cancelText: "取消",
    showCancel: true,
  });
  if (!confirmed) return;
  busy.value = true;
  try {
    status.value = await disconnectCursorAccount();
  } catch (error) {
    await showError("退出登录失败", toUserError(error));
  } finally {
    busy.value = false;
  }
}

onMounted(async () => {
  await refresh().catch(() => {});
  timer = window.setInterval(() => {
    if (waiting.value) void refresh().catch(() => {});
  }, 1500);
});

onUnmounted(() => {
  if (timer) window.clearInterval(timer);
});
</script>

<template>
  <Card>
    <div class="flex items-end justify-between gap-4">
      <div class="min-w-0">
        <div class="flex flex-wrap items-center gap-2">
          <h2 class="text-base font-medium text-white">Cursor 控制面账号</h2>
          <span class="border border-[#3a3a3a] bg-[#202020] px-2 py-0.5 text-xs text-[#b8b8b8]">
            {{ stateText }}
          </span>
        </div>
        <div v-if="signedIn && (status.email || status.authId)" class="mt-1 truncate text-sm text-[#d0d0d0]">
          {{ status.email || status.authId }}
        </div>
        <div class="mt-1 text-sm text-[#a3a3a3]">仅用于插件、Skills 和 MCP，不会改变 Cursor 客户端当前账号</div>
        <div v-if="waiting" class="mt-1 text-sm text-[#d6a84b]">请在浏览器完成登录，然后返回 Cursor 重新打开插件市场</div>
        <div v-if="status.error" class="mt-1 break-all text-sm text-[#e06c75]">{{ status.error }}</div>
      </div>
      <Button v-if="signedIn" class="shrink-0" :disabled="busy" @click="disconnect">退出登录</Button>
      <Button v-else class="shrink-0" variant="primary" :disabled="busy || waiting" @click="login">
        {{ waiting ? "等待登录..." : "登录 Cursor" }}
      </Button>
    </div>
  </Card>
</template>

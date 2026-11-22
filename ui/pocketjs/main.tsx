// SPDX-License-Identifier: GPL-3.0-or-later
// Real PocketJS/QuickJS native UI; no WebView or privileged RPC.
import { createSignal, Show, onCleanup, onMount } from "solid-js";
import { mount } from "@pocketjs/framework/solid";
import { Text, View } from "@pocketjs/framework/solid/components";
import { onButtonPress } from "@pocketjs/framework/lifecycle";
import { BTN } from "@pocketjs/framework/input";
import { createServiceClient, type ServiceMessage } from "@pocketjs/framework/service-client";

type Page = "home" | "library" | "settings" | "overlay";

function Shell() {
  const [page, setPage] = createSignal<Page>("home");
  const [session, setSession] = createSignal("Session: not connected");
  // Real upstream PocketJS v0.17 service-client protocol. The host's
  // OpProvider is the only code that can translate these intents to IPC.
  const svc = createServiceClient("pokshell", { timeoutFrames: 180 });
  let nextRequestId = 1;
  const send = (action: string) => {
    const id = nextRequestId++;
    svc.send({ t: action, id }, (reply: ServiceMessage) => {
      if (reply.t === "result") {
        setSession(String(reply.value ?? "Session: ready"));
      } else {
        setSession("Session: " + String(reply.message ?? "offline"));
      }
    });
  };
  onMount(() => {
    if (svc.transport() === "none") {
      setSession("Session: offline (no native host bridge)");
    } else {
      send("status");
    }
  });
  onCleanup(() => svc.dispose());
  onButtonPress(BTN.TRIANGLE, () => setPage("home"));
  // Console-independent handheld shortcut. The native service still
  // determines which configured executable can be started.
  onButtonPress(BTN.START, () => {
    if (page() === "home" || page() === "library") send("game.start");
  });

  return (
    <View class="w-full h-full flex-col gap-4 p-4 bg-slate-950">
      <View class="flex-col gap-2">
        <Text class="text-xs text-blue-300 font-bold">POKSHELL · P2-J01</Text>
        <Text class="text-2xl text-white font-bold">PocketJS system shell</Text>
        <Text class="text-sm text-slate-300">Native QuickJS + Rust UI. No WebView.</Text>
      </View>
      <Show when={page() === "home"}>
        <View class="flex-col gap-3 grow">
          <Text class="text-lg text-white font-bold">Home</Text>
          <View class="rounded-xl bg-blue-700 p-4 flex-col gap-2"
                focusable onPress={() => send("game.start")}>
            <Text class="text-lg text-white font-bold">Play configured game</Text>
            <Text class="text-sm text-white">START or CIRCLE to launch · no custom executables</Text>
          </View>
          <View class="flex-row gap-3">
            <View class="rounded-xl bg-blue-600 p-4 flex-col gap-2 grow" focusable onPress={() => setPage("library")}>
              <Text class="text-lg text-white font-bold">Library</Text>
              <Text class="text-sm text-white">Browse game details</Text>
            </View>
            <View class="rounded-xl bg-slate-700 p-4 flex-col gap-2 grow" focusable onPress={() => setPage("settings")}>
              <Text class="text-lg text-white font-bold">Settings</Text>
              <Text class="text-sm text-white">System information</Text>
            </View>
          </View>
          <View class="rounded-xl bg-slate-700 p-4 flex-col gap-2" focusable onPress={() => setPage("overlay")}>
            <Text class="text-lg text-white font-bold">Quick menu</Text>
            <Text class="text-sm text-white">Overlay layout prototype</Text>
          </View>
        </View>
      </Show>
      <Show when={page() === "library"}>
        <View class="flex-col gap-3 grow">
          <Text class="text-xl text-white font-bold">Game library</Text>
          <View class="rounded-xl bg-blue-700 p-4 flex-col gap-2">
            <Text class="text-lg text-white font-bold">SuperTux</Text>
            <Text class="text-sm text-white">Native Linux game</Text>
            <Text class="text-sm text-white">Preconfigured game only; no arbitrary executable paths.</Text>
            <View class="rounded-xl bg-slate-700 p-4" focusable onPress={() => send("game.start")}>
              <Text class="text-sm text-white font-bold">Start configured game</Text>
            </View>
            <View class="rounded-xl bg-slate-700 p-4" focusable onPress={() => send("game.stop")}>
              <Text class="text-sm text-white font-bold">Stop game</Text>
            </View>
          </View>
        </View>
      </Show>
      <Show when={page() === "settings"}>
        <View class="flex-col gap-3 grow">
          <Text class="text-xl text-white font-bold">System settings</Text>
          <Text class="text-sm text-slate-300">Display: 800 x 600 Linux host contract</Text>
          <Text class="text-sm text-slate-300">Input: PocketJS button events</Text>
          <Text class="text-sm text-slate-300">Session IPC: bounded PocketJS service bridge</Text>
        </View>
      </Show>
      <Show when={page() === "overlay"}>
        <View class="flex-col gap-3 grow">
          <Text class="text-xl text-white font-bold">Quick menu</Text>
          <Text class="text-sm text-slate-300">Native service intents; hardware focus unverified.</Text>
          <View class="rounded-xl bg-blue-700 p-4" focusable onPress={() => send("overlay.open")}>
            <Text class="text-sm text-white font-bold">Open system overlay</Text>
          </View>
          <View class="rounded-xl bg-slate-700 p-4" focusable onPress={() => send("overlay.close")}>
            <Text class="text-sm text-white font-bold">Close system overlay</Text>
          </View>
        </View>
      </Show>
      <View class="flex-col gap-2">
        <Text class="text-xs text-slate-300">{session()}</Text>
      </View>
      <View class="flex-row justify-between">
        <Text class="text-xs text-slate-300">CIRCLE select · TRIANGLE home</Text>
        <Text class="text-xs text-slate-300">Native PocketJS UI · P2-J01</Text>
      </View>
    </View>
  );
}

mount(() => <Shell />);

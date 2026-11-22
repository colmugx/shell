// SPDX-License-Identifier: GPL-3.0-or-later
// Real PocketJS/QuickJS native UI; no WebView or privileged RPC.
import { createSignal, Show } from "solid-js";
import { mount } from "@pocketjs/framework/solid";
import { Text, View } from "@pocketjs/framework/solid/components";
import { onButtonPress } from "@pocketjs/framework/lifecycle";
import { BTN } from "@pocketjs/framework/input";

type Page = "home" | "library" | "settings" | "overlay";

function Shell() {
  const [page, setPage] = createSignal<Page>("home");
  onButtonPress(BTN.TRIANGLE, () => setPage("home"));

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
            <Text class="text-sm text-white">Launching requires the future session bridge.</Text>
          </View>
        </View>
      </Show>
      <Show when={page() === "settings"}>
        <View class="flex-col gap-3 grow">
          <Text class="text-xl text-white font-bold">System settings</Text>
          <Text class="text-sm text-slate-300">Display: 800 x 600 Linux host contract</Text>
          <Text class="text-sm text-slate-300">Input: PocketJS button events</Text>
          <Text class="text-sm text-slate-300">Session IPC: not connected in P2-J01</Text>
        </View>
      </Show>
      <Show when={page() === "overlay"}>
        <View class="flex-col gap-3 grow">
          <Text class="text-xl text-white font-bold">Quick menu</Text>
          <Text class="text-sm text-slate-300">Local UI only. No game input capture yet.</Text>
        </View>
      </Show>
      <View class="flex-row justify-between">
        <Text class="text-xs text-slate-300">CIRCLE select · TRIANGLE home</Text>
        <Text class="text-xs text-slate-300">Native PocketJS UI · P2-J01</Text>
      </View>
    </View>
  );
}

mount(() => <Shell />);

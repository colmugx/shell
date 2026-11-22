// SPDX-License-Identifier: GPL-3.0-or-later
// Copied into the pinned PocketJS checkout's apps/pokshell before building.
import { defineHostConfig } from "@pocketjs/framework/host-config";

export default defineHostConfig({
  host: "../../programs/linux-kms/pocket.host.json",
  app: {
    id: "org.pokshell.system-ui",
    title: "pokshell",
    version: "0.1.0",
    entry: "main.tsx",
    output: "pokshell-home",
    framework: "solid",
    viewport: { fixed: { logical: [800, 600], presentation: "native" } },
    features: ["text.glyphs.baked", "input.buttons"],
  },
});

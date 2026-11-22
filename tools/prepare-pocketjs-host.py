#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Inject the original pokshell svc OpProvider into *disposable* PocketJS v0.17.

The upstream runtime is not vendored, and none of its public interfaces are
replaced. Reject unexpected revision, duplicate edits or changed anchors.
"""
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
EXPECTED = "83c6cac996237968785125118b543cd4e339254c"


def apply(path: pathlib.Path, old: str, new: str):
    source = path.read_text()
    if source.count(old) != 1:
        raise RuntimeError(f"Unexpected PocketJS v0.17 source layout: {path}")
    path.write_text(source.replace(old, new))


def main():
    upstream = pathlib.Path(sys.argv[1]).resolve()
    revision = subprocess.check_output(["git", "-C", str(upstream), "rev-parse", "HEAD"],
                                       text=True).strip()
    if revision != EXPECTED:
        raise RuntimeError("PocketJS v0.17.0 required: not modifying another revision")
    directory = upstream / "programs/linux-kms"
    target = directory / "src/pokshell_svc.rs"
    if target.exists():
        raise RuntimeError("Refusing to overwrite an existing PocketJS service provider")
    source = ROOT / "ui/pocketjs/host/pokshell_svc.rs"
    shutil.copyfile(source, target)
    cargo = directory / "Cargo.toml"
    apply(cargo,
          'pocket-runtime = { workspace = true, default-features = true }',
          'pocket-runtime = { workspace = true, default-features = true }\n'
          'pocketjs-core = { workspace = true }\nserde_json = "1"')
    main_file = directory / "src/main.rs"
    apply(main_file,
          'use pocket_drivers::{Clock, DisplayOutput, InputClasses, InputFrameBuilder, InputSource};',
          'mod pokshell_svc;\n'
          'use pocket_drivers::{Clock, DisplayOutput, InputClasses, InputFrameBuilder, InputSource};')
    apply(main_file,
          '    let path = options\n        .package',
          '    if let Some(provider) = pokshell_svc::PokShellService::from_environment() {\n'
          '        runtime.add_provider(Box::new(provider)).map_err(error)?;\n'
          '    }\n'
          '    let path = options\n        .package')
    # The optional Wayland backend implements upstream DisplayOutput/Scanout
    # with wl_shm and XDG Shell. It is a normal *client* of Sway/Smithay:
    # the preexisting compositor remains the sole DRM/seat owner.
    display_driver = directory / "src/pokshell_wayland.rs"
    if display_driver.exists():
        raise RuntimeError("Refusing to replace an existing PocketJS Wayland driver")
    shutil.copyfile(ROOT / "ui/pocketjs/host/pokshell_wayland.rs", display_driver)
    apply(cargo,
          'libc = "0.2.191"',
          'libc = "0.2.191"\n'
          'wayland-client = "0.31"\n'
          'wayland-protocols = { version = "0.32", features = ["client"] }\n'
          'tempfile = "3"')
    apply(main_file,
          'mod pokshell_svc;',
          'mod pokshell_svc;\nmod pokshell_wayland;')
    apply(main_file,
          '    pcm_test: Option<String>,',
          '    pcm_test: Option<String>,\n    wayland_display: Option<String>,')
    apply(main_file,
          '            "--headless" => output.headless = true,',
          '            "--headless" => output.headless = true,\n'
          '            "--wayland-display" => {\n'
          '                output.wayland_display = Some(args.next()\n'
          '                    .ok_or_else(|| error("missing --wayland-display socket"))?);\n'
          '            },')
    apply(main_file,
          '    Ok(Some(output))',
          '    if output.wayland_display.is_some() && (output.headless ||\n'
          '        output.card.is_some() || output.fb.is_some() || !output.inputs.is_empty()) {\n'
          '        return Err(error("--wayland-display conflicts with DRM/headless options").into());\n'
          '    }\n'
          '    Ok(Some(output))')
    apply(main_file,
          '    } else {\n        #[cfg(target_os = "linux")]\n        {\n            match pocket_linux_drm::open(',
          '    } else if let Some(display) = options.wayland_display.as_deref() {\n'
          '        let (output, input) = pokshell_wayland::open(size, display)?;\n'
          '        register(&mut runtime, output, size, renderer)?;\n'
          '        runtime.add_input_source(Box::new(input)).map_err(error)?;\n'
          '        eprintln!("display: Wayland/XDG wl_shm (external compositor)");\n'
          '    } else {\n        #[cfg(target_os = "linux")]\n'
          '        {\n            match pocket_linux_drm::open(')

    apply(main_file,
          'pocket-linux-kms [--package app.pkb]',
          'pocket-linux-kms [--package app.pkb] [--wayland-display SOCKET]') 
    # The program adds two *existing upstream workspace* crates as direct
    # dependencies. PocketJS's build executor insists on --locked: update the
    # local disposable checkout's lock metadata once before that check.
    # This does not change or publish anything in pocket-nexus/pocketjs.
    subprocess.run(
        ["cargo", "update", "--manifest-path", str(upstream / "Cargo.toml"),
         "-p", "pocket-linux-kms"],
        cwd=upstream, check=True,
    )
    print("PocketJS native OpProvider registered: svcOpen/svcSend/svcPoll (30..32)")


if __name__ == "__main__":
    main()

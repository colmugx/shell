/* SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include <stdint.h>
#include <stdatomic.h>
#define POCKET_W 480
#define POCKET_H 270
#define POCKET_PIXELS (POCKET_W*POCKET_H)
#define POCKET_SOCK "/run/pocket-session.sock"
#define BTN_LEFT  1u
#define BTN_RIGHT 2u
#define BTN_UP    4u
#define BTN_DOWN  8u
#define BTN_FIRE 16u
typedef struct {
    _Atomic int alive;
    _Atomic uint32_t keys;
    _Atomic uint32_t frame;
    uint32_t pixels[POCKET_PIXELS];
} PocketFrame;

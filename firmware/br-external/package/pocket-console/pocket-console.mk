################################################################################
# SPDX-License-Identifier: GPL-3.0-or-later
# Experimental pokshell alpha Buildroot package
################################################################################
POCKET_CONSOLE_VERSION = 0.1
POCKET_CONSOLE_SITE = $(BR2_EXTERNAL_POKSHELL_PATH)/../..
POCKET_CONSOLE_SITE_METHOD = local
POCKET_CONSOLE_LICENSE = GPL-3.0-or-later
POCKET_CONSOLE_LICENSE_FILES = LICENSE
define POCKET_CONSOLE_BUILD_CMDS
 $(TARGET_CC) $(TARGET_CFLAGS) -std=gnu11 -O2 -I$(@D)/include -o $(@D)/pocket-session $(@D)/src/session.c $(TARGET_LDFLAGS)
 $(TARGET_CC) $(TARGET_CFLAGS) -std=gnu11 -O2 -I$(@D)/include -o $(@D)/pocket-game $(@D)/src/game.c $(TARGET_LDFLAGS)
 $(TARGET_CC) $(TARGET_CFLAGS) -std=gnu11 -O2 -I$(@D)/include -o $(@D)/pocketctl $(@D)/src/pocketctl.c $(TARGET_LDFLAGS)
endef
define POCKET_CONSOLE_INSTALL_TARGET_CMDS
 $(INSTALL) -D -m 0755 $(@D)/pocket-session $(TARGET_DIR)/usr/bin/pocket-session
 $(INSTALL) -D -m 0755 $(@D)/pocket-game $(TARGET_DIR)/usr/bin/pocket-game
 $(INSTALL) -D -m 0755 $(@D)/pocketctl $(TARGET_DIR)/usr/bin/pocketctl
endef
$(eval $(generic-package))

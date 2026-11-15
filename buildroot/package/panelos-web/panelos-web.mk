################################################################################
#
# panelos-web
#
################################################################################

PANELOS_WEB_VERSION = 1.0
PANELOS_WEB_SITE = $(BR2_EXTERNAL_NSPANEL_PATH)/package/panelos-web/src
PANELOS_WEB_SITE_METHOD = local
PANELOS_WEB_LICENSE = AGPL-3.0-or-later
PANELOS_WEB_DEPENDENCIES = wpewebkit wpebackend-fdo libwpe mesa3d wayland host-pkgconf

PANELOS_WEB_PKGS = wpe-webkit-2.0 wpebackend-fdo-1.0 wpe-1.0 egl glesv2 gbm wayland-server

define PANELOS_WEB_BUILD_CMDS
	$(TARGET_CC) $(TARGET_CFLAGS) -std=gnu11 -Wall -Wextra -Wno-unused-parameter \
		$$($(PKG_CONFIG_HOST_BINARY) --cflags $(PANELOS_WEB_PKGS)) \
		-o $(@D)/panelos-web $(@D)/panelos-web.c \
		$(TARGET_LDFLAGS) $$($(PKG_CONFIG_HOST_BINARY) --libs $(PANELOS_WEB_PKGS))
endef

define PANELOS_WEB_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/panelos-web $(TARGET_DIR)/usr/bin/panelos-web
	mkdir -p $(TARGET_DIR)/data
endef

$(eval $(generic-package))

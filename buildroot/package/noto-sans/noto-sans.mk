################################################################################
#
# noto-sans
#
################################################################################

# google/fonts at a fixed commit
NOTO_SANS_VERSION = 8b0a1d0f5983c89bc2b93f1b5fb55f9e252744b5
NOTO_SANS_SITE = https://raw.githubusercontent.com/google/fonts/$(NOTO_SANS_VERSION)/ofl/notosans
NOTO_SANS_SOURCE = NotoSans%5Bwdth,wght%5D.ttf
NOTO_SANS_EXTRA_DOWNLOADS = OFL.txt
NOTO_SANS_LICENSE = OFL-1.1
NOTO_SANS_LICENSE_FILES = OFL.txt

define NOTO_SANS_EXTRACT_CMDS
	cp $(NOTO_SANS_DL_DIR)/$(NOTO_SANS_SOURCE) $(@D)/NotoSans.ttf
	cp $(NOTO_SANS_DL_DIR)/OFL.txt $(@D)/OFL.txt
endef

define NOTO_SANS_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0644 $(@D)/NotoSans.ttf $(TARGET_DIR)/usr/share/fonts/noto/NotoSans.ttf
endef

$(eval $(generic-package))

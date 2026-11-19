# GramMusic cho iOS 5 / 6 (iPhone / iPad / iPod đã jailbreak) — Theos trên Linux, SDK 9.3, armv7 (chạy được A4 trở lên).
# Bộ dựng chép từ GramMini.
#   make package                    bản thử (-DDEBUG, có công cụ thử Debug/)
#   make package FINALPACKAGE=1     bản phát hành
# Cần lõi Rust đã dựng: tools/build-app.sh (hoặc cargo-ios6.sh build --release -p gm-ffi) → CORE_LIB_DIR/libgm_ffi.a
TARGET := iphone:clang:9.3:5.0
ARCHS = armv7
INSTALL_TARGET_PROCESSES = GramMusic

# dpkg của iOS 5 / 6 chỉ đọc .deb nén gzip; nhân iOS 5 / 6 chỉ hiểu chữ ký SHA-1
THEOS_PLATFORM_DEB_COMPRESSION_TYPE = gzip
TARGET_CODESIGN_FLAGS = -S -Hsha1

include $(THEOS)/makefiles/common.mk

APPLICATION_NAME = GramMusic
GramMusic_FILES = main.m $(wildcard Core/*.m) Core/GMShims.c $(wildcard UI/*.m)
ifneq ($(FINALPACKAGE),1)
GramMusic_FILES += $(wildcard Debug/*.m)
endif
GramMusic_FRAMEWORKS = UIKit Foundation CoreGraphics QuartzCore ImageIO AVFoundation AudioToolbox MediaPlayer CoreMedia \
	CFNetwork Security
# API mới hơn iOS 5.0 dùng thẳng là lỗi lúc dựng (TubeMini D24) — chỗ cần iOS 6 thì hỏi lúc chạy (NSClassFromString…)
GramMusic_CFLAGS = -fobjc-arc -Wno-deprecated-declarations -Werror=unguarded-availability -Werror=partial-availability \
	-ICore -IUI -IDebug
# Lõi do tools/build-app.sh dựng ở <thư mục chứa repo>/target-ios (cạnh repo)
CORE_LIB_DIR ?= $(abspath $(CURDIR)/../../target-ios/armv7-apple-ios6/release)
GramMusic_LDFLAGS = -L$(CORE_LIB_DIR) -lgm_ffi -Wl,-dead_strip

include $(THEOS_MAKE_PATH)/application.mk

# Công cụ cài bản cập nhật OTA (Helper/gmupdate.c, chép từ GramMini — DECISIONS M26): quyền root nhờ setuid — layout/DEBIAN/postinst
# đặt chown root + chmod 4755. Quyền com.apple.springboard.launchapplications để mở lại app sau khi cài + khởi động lại SpringBoard.
TOOL_NAME = gmupdate
gmupdate_FILES = Helper/gmupdate.c
gmupdate_FRAMEWORKS = CoreFoundation Security
gmupdate_INSTALL_PATH = /usr/libexec/grammusic
gmupdate_CODESIGN_FLAGS = -SHelper/entitlements.plist -Hsha1
include $(THEOS_MAKE_PATH)/tool.mk

# postinst cần quyền chạy (tệp chép từ Windows không giữ bit x)
before-package::
	chmod 0755 $(THEOS_STAGING_DIR)/DEBIAN/postinst

# Tweak SpringBoard đi kèm: khung điều khiển + lời bài hát trên màn hình khoá iOS 5 / 6 (bật / tắt trong Cài đặt của iPhone → GramLockControl)
# — DECISIONS M20
SUBPROJECTS += ../tweak/GramLockControl
include $(THEOS_MAKE_PATH)/aggregate.mk

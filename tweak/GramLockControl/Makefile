# GramLockControl — tweak nạp vào SpringBoard (MobileSubstrate), đi kèm gói GramMusic (DECISIONS M20). Màn hình khoá iOS 5 / 6 không
# có thanh tiến trình / nút hiện sẵn cho app ngoài → tweak thêm một khung nhỏ phía trên «trượt để mở khoá» khi GramMusic đang phát:
# lời đang hát, thanh tiến trình kéo để tua, nút ⏮ ⏯ ⏭. Bật / tắt trong Cài đặt của iPhone → GramLockControl (PreferenceLoader).
# Dựng cùng app (app/Makefile: SUBPROJECTS); lần đầu cài cần khởi động lại SpringBoard.
TARGET := iphone:clang:9.3:5.0
ARCHS = armv7
include $(THEOS)/makefiles/common.mk
TWEAK_NAME = GramLockControl
GramLockControl_FILES = main.m
GramLockControl_FRAMEWORKS = Foundation UIKit CoreGraphics QuartzCore
GramLockControl_CFLAGS = -fobjc-arc -Wno-deprecated-declarations -Werror=unguarded-availability
GramLockControl_CODESIGN_FLAGS = -S -Hsha1
include $(THEOS_MAKE_PATH)/tweak.mk

# Mục trong Cài đặt của iPhone (PreferenceLoader, chỉ plist — không cần bundle)
internal-stage::
	$(ECHO_NOTHING)mkdir -p $(THEOS_STAGING_DIR)/Library/PreferenceLoader/Preferences$(ECHO_END)
	$(ECHO_NOTHING)cp Prefs/GramLockControl.plist Prefs/GramLockControl.png Prefs/GramLockControl@2x.png \
		$(THEOS_STAGING_DIR)/Library/PreferenceLoader/Preferences/$(ECHO_END)

// gmupdate — cài bản cập nhật (OTA) của GramMusic, chạy quyền root (setuid; postinst đặt chown root + chmod 4755). Chép từ
// gmupdate của GramMini (D33 — đã kiểm bảo mật, thử trọn luồng trên iOS 5.1.1 và iOS 6), gốc là cbupdate của Côn Bằng TV Legacy.
//   /usr/libexec/grammusic/gmupdate <tệp .deb> <pid app> <tệp .sig>
//   /usr/libexec/grammusic/gmupdate --verify <tệp .deb> <tệp .sig>   (chỉ root chạy tay — thử chữ ký trên máy, không cài)
// App tải .deb + chữ ký .deb.sig (tệp đính kèm của GitHub Release) vào /var/mobile/Documents/GramMusic-update/ rồi gọi công cụ
// này. Công cụ có quyền root mà tiến trình nào chạy quyền mobile cũng gọi được → coi mọi thứ bên gọi đưa vào là không tin được
// (kiểm bảo mật GramMini 17/11/2026 — bản trước chỉ kiểm tên gói do chính tệp khai, ai cũng cài được gói tuỳ ý với quyền root):
// - bỏ hết biến môi trường của bên gọi (bash của iOS 5/6 đọc SHELLOPTS / PS4 kể cả khi chạy root — postinst là sh), umask 022,
//   đóng các tệp bên gọi để mở;
// - chỉ nhận lời gọi từ chính app (tiến trình cha là /Applications/GramMusic.app/GramMusic) và chỉ đóng đúng tiến trình đó;
// - chép .deb vào thư mục chỉ root đọc ghi (/var/root/GramMusic-update), băm SHA-256 lúc chép, kiểm chữ ký RSA PKCS#1 v1.5
//   (khoá công khai gắn sẵn — ota_cert.h; khoá riêng chỉ ở máy phát hành) trên chính bản chép, rồi cài bản chép → không ai đổi
//   được tệp giữa lúc kiểm và lúc cài;
// - đúng gói vn.grammusic.app, số bản mới hơn bản đang cài (không cho cài lại bản cũ có lỗi đã sửa);
// - nhật ký trong thư mục của root, không theo liên kết tượng trưng; không xoá / ghi gì bằng quyền root trong thư mục của mobile.
// Kiểm xong thì tách nền (app nhận mã thoát ngay), đóng app, dpkg -i, rồi LUÔN khởi động lại SpringBoard (anh yêu cầu: cập nhật
// xong tự respring — tweak GramLockControl trong gói chỉ chạy lại sau respring; biểu tượng / tên mới cũng cần) và mở lại app
// (SpringBoardServices, quyền com.apple.springboard.launchapplications). iOS 5: sau respring máy về màn khoá → mở lại app
// không được (SpringBoardServices báo lỗi 3 — Côn Bằng TV đã thử trên iPad 1) → thử một lúc rồi thôi, người dùng mở khoá là
// thấy bản mới.
// dpkg chạy với GRAMMUSIC_OTA=1 để postinst biết công cụ này tự respring (không xin Cydia nút «Restart SpringBoard»).
// Nhật ký: /var/root/GramMusic-update/install.log (đọc qua SSH bằng root).
#include <CommonCrypto/CommonDigest.h>
#include <CoreFoundation/CoreFoundation.h>
#include <Security/Security.h>
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <signal.h>
#include <spawn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/param.h>
#include <sys/resource.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

#include "ota_cert.h"

#define UPDATE_DIR "/var/mobile/Documents/GramMusic-update/"
#define ROOT_DIR "/var/root/GramMusic-update"
#define COPY_PATH ROOT_DIR "/update.deb"
#define APP_EXE "/Applications/GramMusic.app/GramMusic"
#define PACKAGE "vn.grammusic.app"
#define MOBILE_UID 501
#define MAX_DEB (96LL * 1024 * 1024)
#define MAX_SIG 1024

extern char **environ;
int proc_pidpath(int pid, void *buffer, uint32_t buffersize);  // libproc (có trên iOS 5 / 6, SDK không kèm libproc.h)

// Môi trường cố định cho mọi lệnh con chạy root — không lấy gì từ bên gọi
static char *gEnv[] = {"PATH=/usr/bin:/bin:/usr/sbin:/sbin", "HOME=/var/root", "GRAMMUSIC_OTA=1", NULL};
static char *gMobileEnv[] = {"PATH=/usr/bin:/bin:/usr/sbin:/sbin", "HOME=/var/mobile", NULL};

static void logf_(const char *fmt, const char *a) {
    time_t t = time(NULL);
    char ts[32];
    strftime(ts, sizeof(ts), "%m-%d %H:%M:%S", localtime(&t));
    printf("%s ", ts);
    printf(fmt, a ? a : "");
    printf("\n");
    fflush(stdout);
}

/// Chạy lệnh (môi trường cố định), chờ xong, trả mã thoát (-1 = không chạy được). out: nhận stdout (có thể NULL)
static int run(char *const argv[], char *out, size_t outSize) {
    int fds[2] = {-1, -1};
    posix_spawn_file_actions_t fa;
    posix_spawn_file_actions_init(&fa);
    if (out) {
        if (pipe(fds) != 0) return -1;
        posix_spawn_file_actions_adddup2(&fa, fds[1], 1);
        posix_spawn_file_actions_addclose(&fa, fds[0]);
    }
    pid_t pid;
    int rc = posix_spawn(&pid, argv[0], &fa, NULL, argv, gEnv);
    posix_spawn_file_actions_destroy(&fa);
    if (out) close(fds[1]);
    if (rc != 0) {
        if (out) close(fds[0]);
        return -1;
    }
    if (out) {
        size_t n = 0;
        ssize_t r;
        while (n + 1 < outSize && (r = read(fds[0], out + n, outSize - 1 - n)) > 0) n += (size_t)r;
        out[n] = 0;
        close(fds[0]);
    }
    int status = 0;
    while (waitpid(pid, &status, 0) < 0 && errno == EINTR) {
    }
    return WIFEXITED(status) ? WEXITSTATUS(status) : -1;
}

/// Tên hiện dưới biểu tượng (CFBundleDisplayName / CFBundleName trong Info.plist dạng XML); rỗng nếu không đọc được
static void displayName(char *out, size_t size) {
    out[0] = 0;
    FILE *f = fopen("/Applications/GramMusic.app/Info.plist", "r");
    if (!f) return;
    char buf[16384];
    size_t n = fread(buf, 1, sizeof(buf) - 1, f);
    fclose(f);
    buf[n] = 0;
    char *k = strstr(buf, "<key>CFBundleDisplayName</key>");
    if (!k) k = strstr(buf, "<key>CFBundleName</key>");
    char *s = k ? strstr(k, "<string>") : NULL;
    char *e = s ? strstr(s, "</string>") : NULL;
    if (!s || !e) return;
    s += 8;
    size_t len = (size_t)(e - s) < size - 1 ? (size_t)(e - s) : size - 1;
    memcpy(out, s, len);
    out[len] = 0;
}

static int launchApp(void) {
    void *sbs = dlopen("/System/Library/PrivateFrameworks/SpringBoardServices.framework/SpringBoardServices", RTLD_LAZY);
    if (!sbs) return -1;
    CFStringRef bundleId = CFSTR(PACKAGE);
    int (*launchWithOptions)(CFStringRef, CFDictionaryRef, Boolean) =
        (int (*)(CFStringRef, CFDictionaryRef, Boolean))dlsym(sbs, "SBSLaunchApplicationWithIdentifierAndLaunchOptions");
    int result = -1;
    if (launchWithOptions) {
        CFStringRef *unlockKey = (CFStringRef *)dlsym(sbs, "SBSApplicationLaunchOptionUnlockDeviceKey");
        CFStringRef key = unlockKey && *unlockKey ? *unlockKey : CFSTR("__UnlockDevice");
        const void *keys[] = {key};
        const void *values[] = {kCFBooleanTrue};
        CFDictionaryRef options = CFDictionaryCreate(NULL, keys, values, 1, &kCFTypeDictionaryKeyCallBacks,
                                                     &kCFTypeDictionaryValueCallBacks);
        result = launchWithOptions(bundleId, options, false);
        CFRelease(options);
    }
    if (result != 0) {
        int (*launchPlain)(CFStringRef, Boolean) = (int (*)(CFStringRef, Boolean))dlsym(sbs, "SBSLaunchApplicationWithIdentifier");
        if (launchPlain) result = launchPlain(bundleId, false);
    }
    return result;
}

/// App đang ở trước màn hình?
static int appIsFrontmost(void) {
    void *sbs = dlopen("/System/Library/PrivateFrameworks/SpringBoardServices.framework/SpringBoardServices", RTLD_LAZY);
    CFStringRef (*copyFront)(void) = sbs ? (CFStringRef (*)(void))dlsym(sbs, "SBSCopyFrontmostApplicationDisplayIdentifier") : NULL;
    if (!copyFront) return 0;
    CFStringRef front = copyFront();
    int yes = front && CFStringCompare(front, CFSTR(PACKAGE), 0) == kCFCompareEqualTo;
    if (front) CFRelease(front);
    return yes;
}

/// Khởi động lại SpringBoard rồi mở lại app. SpringBoard lên lại mất ~15–30 giây (lúc đầu nhận lệnh mở app mà chưa mở thật) →
/// thử tới khi app thật sự ở trước màn hình (iOS 6: tối đa ~90 giây; iOS 5: ~30 giây — máy về màn khoá, thường không mở được)
static int respringAndRelaunch(int tries) {
    char *respring[] = {"/usr/bin/killall", "-9", "SpringBoard", NULL};
    run(respring, NULL, 0);
    logf_("đã khởi động lại SpringBoard%s", "");
    sleep(12);
    for (int i = 0; i < tries; i++) {
        if (launchApp() == 0) {
            sleep(2);
            if (appIsFrontmost()) return 0;
        } else {
            sleep(2);
        }
    }
    return -1;
}

/// Tệp bên gọi đưa: đúng thư mục tải của app, đúng đuôi, tệp thường (realpath gỡ hết liên kết tượng trưng — trên iOS /var là
/// liên kết tới /private/var nên so với realpath của chính thư mục tải). Chỉ là lọc sơ bộ: nội dung được chép + kiểm chữ ký sau.
static int validIn(const char *arg, const char *ext, char real[PATH_MAX]) {
    char dir[PATH_MAX];
    if (!realpath(arg, real) || !realpath(UPDATE_DIR, dir)) return 0;
    strlcat(dir, "/", PATH_MAX);
    size_t n = strlen(real), dn = strlen(dir), en = strlen(ext);
    struct stat st;
    if (strncmp(real, dir, dn) != 0 || n < dn + en + 1 || strcmp(real + n - en, ext) != 0 || strchr(real + dn, '/') ||
        stat(real, &st) != 0 || !S_ISREG(st.st_mode)) {
        fprintf(stderr, "tệp không hợp lệ\n");
        return 0;
    }
    return 1;
}

/// Tiến trình `pid` là chính GramMusic (đường dẫn tệp chạy — /Applications có thể là liên kết tới /var/stash nên so realpath)
static int isApp(pid_t pid) {
    char want[PATH_MAX], got[4 * MAXPATHLEN];
    if (pid <= 1 || !realpath(APP_EXE, want)) return 0;
    memset(got, 0, sizeof(got));
    if (proc_pidpath(pid, got, sizeof(got)) <= 0) return 0;
    return strcmp(got, want) == 0;
}

/// Thư mục làm việc của root: /var/root/GramMusic-update, chủ root, 0700, không phải liên kết tượng trưng
static int prepareRootDir(void) {
    mkdir(ROOT_DIR, 0700);
    struct stat st;
    if (lstat(ROOT_DIR, &st) != 0 || !S_ISDIR(st.st_mode) || st.st_uid != 0) return 0;
    chmod(ROOT_DIR, 0700);
    return 1;
}

static int writeAll(int fd, const char *p, size_t n) {
    while (n) {
        ssize_t w = write(fd, p, n);
        if (w < 0 && errno == EINTR) continue;
        if (w <= 0) return 0;
        p += w;
        n -= (size_t)w;
    }
    return 1;
}

/// Chép `src` (tệp thường, không theo liên kết, ≤ MAX_DEB) sang `dst` (tạo mới, 0600) và băm SHA-256 đúng những byte đã chép
static int copyAndHash(const char *src, const char *dst, unsigned char hash[CC_SHA256_DIGEST_LENGTH]) {
    int in = open(src, O_RDONLY | O_NOFOLLOW);
    if (in < 0) return 0;
    struct stat st;
    if (fstat(in, &st) != 0 || !S_ISREG(st.st_mode) || st.st_size <= 0 || st.st_size > MAX_DEB) {
        close(in);
        return 0;
    }
    unlink(dst);
    int out = open(dst, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, 0600);
    if (out < 0) {
        close(in);
        return 0;
    }
    CC_SHA256_CTX c;
    CC_SHA256_Init(&c);
    static char buf[65536];
    long long total = 0;
    int ok = 1;
    ssize_t r;
    while ((r = read(in, buf, sizeof(buf))) != 0) {
        if (r < 0) {
            if (errno == EINTR) continue;
            ok = 0;
            break;
        }
        total += r;
        if (total > MAX_DEB || !writeAll(out, buf, (size_t)r)) {
            ok = 0;
            break;
        }
        CC_SHA256_Update(&c, buf, (CC_LONG)r);
    }
    close(in);
    if (close(out) != 0) ok = 0;
    CC_SHA256_Final(hash, &c);
    if (!ok) unlink(dst);
    return ok;
}

/// Đọc tệp nhỏ (chữ ký) — tệp thường, không theo liên kết; trả số byte (0 = lỗi)
static size_t readSmall(const char *path, unsigned char *buf, size_t size) {
    int fd = open(path, O_RDONLY | O_NOFOLLOW);
    if (fd < 0) return 0;
    struct stat st;
    if (fstat(fd, &st) != 0 || !S_ISREG(st.st_mode) || st.st_size <= 0 || (size_t)st.st_size > size) {
        close(fd);
        return 0;
    }
    size_t n = 0;
    ssize_t r;
    while (n < size && (r = read(fd, buf + n, size - n)) > 0) n += (size_t)r;
    close(fd);
    return n;
}

/// Chữ ký RSA PKCS#1 v1.5 + SHA-256 (openssl dgst -sha256 -sign) của khoá trong ota_cert.h. iOS 5 / 6 không nạp được khoá RSA
/// trần → đi qua chứng chỉ: SecTrust lấy khoá công khai (chứng chỉ tự ký — kết quả đánh giá tin cậy không quan trọng, chỉ cần
/// khoá); kSecPaddingPKCS1 + DigestInfo SHA-256 tự ghép (kSecPaddingPKCS1SHA256 không chắc có trên iOS 5).
static int verifySig(const unsigned char hash[CC_SHA256_DIGEST_LENGTH], const unsigned char *sig, size_t sigLen) {
    static const unsigned char kDigestInfo[19] = {0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01,
                                                  0x65, 0x03, 0x04, 0x02, 0x01, 0x05, 0x00, 0x04, 0x20};
    int ok = 0;
    CFDataRef der = CFDataCreate(NULL, kOtaCert, sizeof(kOtaCert));
    SecCertificateRef cert = der ? SecCertificateCreateWithData(NULL, der) : NULL;
    SecPolicyRef policy = SecPolicyCreateBasicX509();
    SecTrustRef trust = NULL;
    SecKeyRef key = NULL;
    if (cert && policy && SecTrustCreateWithCertificates(cert, policy, &trust) == errSecSuccess && trust) {
        SecTrustResultType result;
        SecTrustEvaluate(trust, &result);
        key = SecTrustCopyPublicKey(trust);
    }
    if (key) {
        unsigned char signedData[sizeof(kDigestInfo) + CC_SHA256_DIGEST_LENGTH];
        memcpy(signedData, kDigestInfo, sizeof(kDigestInfo));
        memcpy(signedData + sizeof(kDigestInfo), hash, CC_SHA256_DIGEST_LENGTH);
        ok = SecKeyRawVerify(key, kSecPaddingPKCS1, signedData, sizeof(signedData), sig, sigLen) == errSecSuccess;
        CFRelease(key);
    } else {
        fprintf(stderr, "không đọc được khoá công khai\n");
    }
    if (trust) CFRelease(trust);
    if (policy) CFRelease(policy);
    if (cert) CFRelease(cert);
    if (der) CFRelease(der);
    return ok;
}

/// Trường trong control của .deb (dpkg-deb -f); rỗng nếu lỗi
static void debField(const char *deb, const char *name, char *out, size_t size) {
    char *argv[] = {"/usr/bin/dpkg-deb", "-f", (char *)deb, (char *)name, NULL};
    if (run(argv, out, size) != 0) out[0] = 0;
    out[strcspn(out, "\r\n")] = 0;
}

/// Bản trong .deb mới hơn bản đang cài (dpkg --compare-versions); chưa cài / không đọc được bản đang cài → cho cài
static int newerThanInstalled(const char *ver) {
    char cur[128] = {0};
    char *query[] = {"/usr/bin/dpkg-query", "-W", "--showformat=${Version}", PACKAGE, NULL};
    if (run(query, cur, sizeof(cur)) != 0) return 1;
    cur[strcspn(cur, "\r\n")] = 0;
    if (!cur[0]) return 1;
    char *cmp[] = {"/usr/bin/dpkg", "--compare-versions", (char *)ver, "gt", cur, NULL};
    return run(cmp, NULL, 0) == 0;
}

/// Chép + kiểm chữ ký + đúng gói. 0 = được; khác 0 = mã lỗi (bản chép đã xoá). ver: số bản trong gói
static int checkUpdate(const char *deb, const char *sigPath, char *ver, size_t verSize) {
    unsigned char hash[CC_SHA256_DIGEST_LENGTH], sig[MAX_SIG];
    if (!prepareRootDir() || !copyAndHash(deb, COPY_PATH, hash)) {
        fprintf(stderr, "không chép được tệp cập nhật\n");
        return 71;
    }
    size_t sigLen = readSmall(sigPath, sig, sizeof(sig));
    if (!sigLen || !verifySig(hash, sig, sigLen)) {
        fprintf(stderr, "chữ ký không đúng\n");
        unlink(COPY_PATH);
        return 72;
    }
    char pkg[256];
    debField(COPY_PATH, "Package", pkg, sizeof(pkg));
    if (strcmp(pkg, PACKAGE) != 0) {
        fprintf(stderr, "gói không phải " PACKAGE "\n");
        unlink(COPY_PATH);
        return 68;
    }
    debField(COPY_PATH, "Version", ver, verSize);
    if (!ver[0]) {
        unlink(COPY_PATH);
        return 68;
    }
    return 0;
}

/// Tách nền (cha thoát ngay), ghi nhật ký vào thư mục của root. Trả 1 ở tiến trình con, 0 ở cha, -1 lỗi
static int detach(void) {
    pid_t child = fork();
    if (child < 0) return -1;
    if (child > 0) return 0;
    setsid();
    int logfd = open(ROOT_DIR "/install.log", O_WRONLY | O_CREAT | O_APPEND | O_NOFOLLOW, 0600);
    if (logfd >= 0) {
        dup2(logfd, 1);
        dup2(logfd, 2);
        close(logfd);
    }
    int devnull = open("/dev/null", O_RDONLY);
    if (devnull >= 0) dup2(devnull, 0);
    return 1;
}

int main(int argc, char **argv) {
    // 0. Không mang gì của bên gọi theo: môi trường (trước mọi lệnh CoreFoundation / Security), umask, tệp đang mở, giới hạn
    environ = gEnv;
    umask(022);
    for (int fd = 3; fd < 256; fd++) close(fd);
    // Thử chữ ký trên máy (root chạy tay qua SSH): không cài gì
    if (argc == 4 && strcmp(argv[1], "--verify") == 0) {
        if (getuid() != 0) return 64;
        char ver[128] = {0};
        int rc = checkUpdate(argv[2], argv[3], ver, sizeof(ver));
        unlink(COPY_PATH);
        if (rc == 0) printf("chữ ký đúng, gói " PACKAGE " %s\n", ver);
        else printf("không đạt (mã %d)\n", rc);
        return rc;
    }
    if (argc < 4) {
        fprintf(stderr, "cách dùng: gmupdate <tệp .deb> <pid app> <tệp .sig> (tệp trong " UPDATE_DIR ")\n");
        return 64;
    }
    // 1. Tệp: đúng thư mục tải của app (lọc sơ bộ)
    char real[PATH_MAX], sigReal[PATH_MAX];
    if (!validIn(argv[1], ".deb", real) || !validIn(argv[3], ".sig", sigReal)) return 65;
    // 2. Quyền root (bit setuid do postinst đặt) — đổi cả uid thật: tiến trình mobile không gửi tín hiệu tới được nữa
    if (setgid(0) != 0 || setuid(0) != 0) {
        fprintf(stderr, "gmupdate cần quyền root (setuid) — cài lại gói " PACKAGE "\n");
        return 66;
    }
    struct rlimit inf = {RLIM_INFINITY, RLIM_INFINITY};
    setrlimit(RLIMIT_FSIZE, &inf);
    setrlimit(RLIMIT_CPU, &inf);
    // 3. Chỉ chính app được gọi (bỏ pid bên gọi đưa — chỉ đóng đúng tiến trình cha nếu là GramMusic)
    pid_t app = getppid();
    if (!isApp(app) || atoi(argv[2]) != app) {
        fprintf(stderr, "chỉ GramMusic được gọi công cụ này\n");
        return 69;
    }
    // 4. Chép vào thư mục của root, kiểm chữ ký, đúng gói, bản mới hơn
    char ver[128] = {0};
    int bad = checkUpdate(real, sigReal, ver, sizeof(ver));
    if (bad) return bad;
    if (!newerThanInstalled(ver)) {
        fprintf(stderr, "bản %s không mới hơn bản đang cài\n", ver);
        unlink(COPY_PATH);
        return 73;
    }
    // 5. Tách nền: app nhận mã 0 ngay (đã kiểm xong), phần cài chạy tiếp sau khi app bị đóng
    int d = detach();
    if (d <= 0) return d == 0 ? 0 : 70;
    logf_("cài bản %s", ver);
    sleep(1);
    kill(app, SIGTERM);
    for (int i = 0; i < 30 && kill(app, 0) == 0; i++) usleep(100000);
    if (kill(app, 0) == 0 && isApp(app)) kill(app, SIGKILL);
    logf_("đã đóng app%s", "");
    char before[256], after[256];
    displayName(before, sizeof(before));
    char *install[] = {"/usr/bin/dpkg", "-i", COPY_PATH, NULL};
    int rc = run(install, NULL, 0);  // gEnv có GRAMMUSIC_OTA=1: postinst biết công cụ này tự khởi động lại SpringBoard
    char code[16];
    snprintf(code, sizeof(code), "%d", rc);
    logf_("dpkg -i → %s", code);
    unlink(COPY_PATH);  // tệp tải trong thư mục của mobile: app tự dọn (không xoá bằng quyền root ở đó)
    displayName(after, sizeof(after));
    // Không uicache thường xuyên (iOS 6 có lúc làm SpringBoard treo — Côn Bằng TV, iPad mini 1); chỉ khi đổi tên hiện dưới biểu
    // tượng. Bước khởi động lại SpringBoard ngay sau đó gỡ được nếu có treo.
    if (rc == 0 && strcmp(before, after) != 0) {
        logf_("đổi tên app → %s: uicache", after);
        pid_t u = fork();
        if (u == 0) {
            if (setgid(MOBILE_UID) != 0 || setuid(MOBILE_UID) != 0) _exit(126);
            execle("/usr/bin/uicache", "uicache", (char *)NULL, gMobileEnv);
            _exit(127);
        }
        if (u > 0) {
            int status;
            for (int i = 0; i < 300 && waitpid(u, &status, WNOHANG) == 0; i++) usleep(100000);  // tối đa 30 giây
            kill(u, SIGKILL);
        }
    }
    int launched;
    if (rc == 0) {
        // Cài xong: luôn khởi động lại SpringBoard (GramLockControl, biểu tượng / tên mới — postinst đã xoá bộ đệm biểu tượng) rồi mở lại app
        int oldIOS = kCFCoreFoundationVersionNumber < 793.0;  // 793.00 = iOS 6.0
        launched = respringAndRelaunch(oldIOS ? 9 : 39);
    } else {
        // Cài lỗi: bản cũ vẫn nguyên, chỉ mở lại app
        sleep(1);
        launched = launchApp();
        if (launched != 0) {  // SpringBoard đang bận: thử lại một lần
            sleep(3);
            launched = launchApp();
        }
    }
    snprintf(code, sizeof(code), "%d", launched);
    logf_("mở lại app → %s", code);
    return rc == 0 ? 0 : 1;
}

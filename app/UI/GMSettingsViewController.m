#import "GMSettingsViewController.h"
#import "GMAppDelegate.h"
#import "GMCore.h"
#import "GMImages.h"
#import "GMProxyViewController.h"
#import "GMRootController.h"
#import "GMTheme.h"

/// Bảng chọn dùng khối (UIActionSheet không giữ delegate).
@interface GMChoice : NSObject <UIActionSheetDelegate, UIAlertViewDelegate>
@property (nonatomic, copy) void (^pick)(NSInteger i);
@end

static NSMutableSet *gChoices;

@implementation GMChoice
- (void)actionSheet:(UIActionSheet *)s clickedButtonAtIndex:(NSInteger)i {
    if (i != s.cancelButtonIndex && i >= 0 && _pick) _pick(i);
}
- (void)actionSheet:(UIActionSheet *)s didDismissWithButtonIndex:(NSInteger)i {
    [gChoices removeObject:self];
}
- (void)alertView:(UIAlertView *)a clickedButtonAtIndex:(NSInteger)i {
    if (i != a.cancelButtonIndex && _pick) _pick(i);
}
- (void)alertView:(UIAlertView *)a didDismissWithButtonIndex:(NSInteger)i {
    [gChoices removeObject:self];
}
@end

static void GMChoose(UIViewController *vc, NSString *title, NSArray *options, void (^pick)(NSInteger i)) {
    if (!gChoices) gChoices = [NSMutableSet set];
    GMChoice *c = [[GMChoice alloc] init];
    c.pick = pick;
    UIActionSheet *s = [[UIActionSheet alloc] initWithTitle:title delegate:c cancelButtonTitle:nil destructiveButtonTitle:nil
                                          otherButtonTitles:nil];
    for (NSString *o in options) [s addButtonWithTitle:o];
    s.cancelButtonIndex = [s addButtonWithTitle:@"Huỷ"];
    [gChoices addObject:c];
    [s showInView:vc.view.window ?: vc.view];
}

static void GMConfirm(NSString *title, NSString *msg, NSString *ok, void (^yes)(void)) {
    if (!gChoices) gChoices = [NSMutableSet set];
    GMChoice *c = [[GMChoice alloc] init];
    c.pick = ^(NSInteger i) { yes(); };
    [gChoices addObject:c];
    [[[UIAlertView alloc] initWithTitle:title message:msg delegate:c cancelButtonTitle:@"Huỷ" otherButtonTitles:ok, nil] show];
}

#pragma mark - Cài đặt

/// Các nhóm của màn Cài đặt.
enum { GMSecAccount, GMSecLibrary, GMSecPlayback, GMSecApp, GMSecAbout, GMSecLogout, GMSecCount };

/// «Tải trước»: số giây phía trước chỗ đang nghe (0 = cả bài).
static NSArray *GMAheadValues(void) {
    return @[@30, @60, @120, @300, @0];
}

static NSString *GMAheadText(NSInteger secs) {
    if (secs <= 0) return @"Cả bài";
    if (secs < 60) return [NSString stringWithFormat:@"%ld giây", (long)secs];
    return [NSString stringWithFormat:@"%ld phút", (long)(secs / 60)];
}

@implementation GMSettingsViewController {
    NSDictionary *_stats;
    NSDictionary *_prefs;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Cài đặt";
}

- (void)viewWillAppear:(BOOL)animated {
    [super viewWillAppear:animated];
    [GMCore call:@"library.stats" args:nil done:^(id ok, NSDictionary *err) {
        if (!err) _stats = ok;
        [self.tableView reloadData];
    }];
    [self loadPrefs];
}

- (void)loadPrefs {
    [GMCore call:@"settings.get" args:nil done:^(id ok, NSDictionary *err) {
        if (!err) _prefs = ok;
        [self.tableView reloadData];
    }];
}

- (void)setPref:(NSString *)key value:(id)v {
    [GMCore call:@"settings.set" args:@{key: v} done:^(id ok, NSDictionary *err) {
        if (err) [GMTheme toast:GMErrorText(err) in:self.view];
        [self loadPrefs];
    }];
}

- (NSInteger)numberOfSectionsInTableView:(UITableView *)tv {
    return GMSecCount;
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    switch (s) {
        case GMSecAccount: return 1;
        case GMSecLibrary: return 2;
        case GMSecPlayback: return 2;
        case GMSecApp: return 4;
        case GMSecAbout: return 1;
        default: return GMRoot().me ? 1 : 0;
    }
}

- (NSString *)tableView:(UITableView *)tv titleForHeaderInSection:(NSInteger)s {
    if (s == GMSecLibrary) return @"Thư viện";
    if (s == GMSecPlayback) return @"Phát nhạc";
    return nil;
}

- (NSString *)tableView:(UITableView *)tv titleForFooterInSection:(NSInteger)s {
    if (s == GMSecPlayback) {
        return @"Tải trước: khi nghe chỉ tải chừng này phía trước chỗ đang nghe, tạm dừng thì thôi tải — tệp lớn đỡ tốn dung lượng mạng. "
               @"«Cả bài»: tải hết bài ngay khi nghe. Lời bài hát lấy từ LRCLIB (lrclib.net).";
    }
    if (s == GMSecApp) {
        NSString *prov = GMVal(_prefs, @"dns_providers") ?: @"Cloudflare → Google";
        return [NSString stringWithFormat:@"DNS mã hoá: tên miền của proxy, lời bài hát… được hỏi qua %@ (DoH, rồi DoT) — nhà mạng "
                                          @"không đọc / sửa được; lỗi hết mới dùng DNS của máy.", prov];
    }
    if (s == GMSecLibrary) {
        NSInteger total = [GMVal(_stats, @"total") integerValue];
        NSNumber *last = GMVal(_stats, @"last_sync");
        NSString *when = last.longLongValue > 0 ? [NSString stringWithFormat:@", quét lần cuối %@ %@", [GMTheme shortTime:last],
                                                   [GMTheme clock:last]] : @"";
        NSString *partial = [GMVal(_stats, @"partial") boolValue] ? @" Lần quét toàn bộ trước còn dở — sẽ quét tiếp." : @"";
        return [NSString stringWithFormat:@"%ld bài từ %@ nguồn%@.%@", (long)total, GMVal(_stats, @"sources") ?: @0, when, partial];
    }
    if (s == GMSecAbout) {
        return @"GramMusic — nghe nhạc từ Telegram trên iOS 5 / 6. Ứng dụng không chính thức, dùng Telegram API; không liên kết với "
               @"Telegram. Phiên đăng nhập và thư viện chỉ nằm trên máy này.";
    }
    return nil;
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    UITableViewCell *c = [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleValue1 reuseIdentifier:nil];
    c.accessoryType = UITableViewCellAccessoryDisclosureIndicator;
    if (ip.section == GMSecAccount) {
        NSDictionary *me = GMRoot().me;
        if (me) {
            c = [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleSubtitle reuseIdentifier:nil];
            c.textLabel.text = GMVal(me, @"name");
            NSString *u = GMVal(me, @"username");
            NSString *phone = GMVal(me, @"phone");
            c.detailTextLabel.text = u.length ? [@"@" stringByAppendingString:u] : (phone.length ? [@"+" stringByAppendingString:phone] : @"");
            c.imageView.image = [GMTheme avatarFor:GMVal(me, @"id") name:GMVal(me, @"name") size:40];
            NSString *photo = GMVal(me, @"photo");
            if (photo) {
                __weak UITableViewCell *wc = c;
                [GMImages avatarForPeer:GMVal(me, @"id") photo:photo dc:GMVal(me, @"photo_dc") size:40 done:^(UIImage *img) {
                    wc.imageView.image = img;
                    [wc setNeedsLayout];
                }];
            }
            c.selectionStyle = UITableViewCellSelectionStyleNone;
            c.accessoryType = UITableViewCellAccessoryNone;
        } else {
            c.textLabel.text = @"Đăng nhập Telegram";
            c.textLabel.textColor = [GMTheme accent];
        }
    } else if (ip.section == GMSecLibrary) {
        c.textLabel.text = ip.row == 0 ? @"Lấy bài mới" : @"Quét toàn bộ tài khoản";
        c.imageView.image = [GMTheme settingsIcon:ip.row == 0 ? @"folders" : @"saved"];
        c.accessoryType = UITableViewCellAccessoryNone;
    } else if (ip.section == GMSecPlayback) {
        if (ip.row == 0) {
            c.textLabel.text = @"Tải trước";
            c.detailTextLabel.text = _prefs ? GMAheadText([GMVal(_prefs, @"ahead_secs") integerValue]) : @"";
            c.imageView.image = [GMTheme settingsIcon:@"ahead"];
        } else {
            c.textLabel.text = @"Lời bài hát";
            c.detailTextLabel.text = !_prefs ? @"" : ([GMVal(_prefs, @"lyrics") boolValue] ? @"Bật" : @"Tắt");
            c.imageView.image = [GMTheme settingsIcon:@"lyrics"];
        }
    } else if (ip.section == GMSecApp) {
        if (ip.row == 0) {
            c.textLabel.text = @"Proxy";
            c.imageView.image = [GMTheme settingsIcon:@"proxy"];
        } else if (ip.row == 1) {
            c.textLabel.text = @"DNS mã hoá";
            c.detailTextLabel.text = !_prefs ? @"" : ([GMVal(_prefs, @"secure_dns") boolValue] ? @"Bật" : @"Tắt");
            c.imageView.image = [GMTheme settingsIcon:@"lock"];
        } else if (ip.row == 2) {
            c.textLabel.text = @"Dữ liệu và bộ nhớ";
            c.imageView.image = [GMTheme settingsIcon:@"storage"];
        } else {
            c.textLabel.text = @"Giao diện";
            c.detailTextLabel.text = [GMTheme modeText:[GMTheme mode]];
            c.imageView.image = [GMTheme settingsIcon:@"theme"];
        }
    } else if (ip.section == GMSecAbout) {
        c.textLabel.text = @"Phiên bản";
        c.detailTextLabel.text = [[NSBundle mainBundle] objectForInfoDictionaryKey:@"CFBundleShortVersionString"];
        c.imageView.image = [GMTheme settingsIcon:@"info"];
        c.accessoryType = UITableViewCellAccessoryNone;
        c.selectionStyle = UITableViewCellSelectionStyleNone;
    } else {
        c = [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleDefault reuseIdentifier:nil];
        c.textLabel.text = @"Đăng xuất";
        c.textLabel.textAlignment = GMAlignCenter;
        c.textLabel.textColor = [UIColor colorWithRed:0.85 green:0.2 blue:0.2 alpha:1];
    }
    return c;
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    GMAppDelegate *app = (GMAppDelegate *)[UIApplication sharedApplication].delegate;
    if (ip.section == GMSecAccount && !GMRoot().me) {
        [app showLogin];
    } else if (ip.section == GMSecPlayback && ip.row == 0) {
        NSMutableArray *names = [NSMutableArray array];
        for (NSNumber *v in GMAheadValues()) {
            NSString *n = GMAheadText(v.integerValue);
            [names addObject:v.integerValue == 60 ? [n stringByAppendingString:@" (mặc định)"] : n];
        }
        GMChoose(self, @"Tải trước bao lâu phía trước chỗ đang nghe?", names, ^(NSInteger i) {
            [self setPref:@"ahead_secs" value:[GMAheadValues() objectAtIndex:i]];
        });
    } else if (ip.section == GMSecPlayback) {
        if ([GMVal(_prefs, @"lyrics") boolValue]) {
            [self setPref:@"lyrics" value:@NO];
        } else {
            GMConfirm(@"Bật lời bài hát?", @"Khi mở màn Đang phát, tên bài, ca sĩ và thời lượng của bài được gửi tới LRCLIB (lrclib.net — "
                                          @"kho lời bài hát miễn phí) để tìm lời. Không gửi gì khác.", @"Bật", ^{
                [self setPref:@"lyrics" value:@YES];
            });
        }
    } else if (ip.section == GMSecLibrary) {
        if (ip.row == 0) {
            [GMCore call:@"sync.start" args:@{} done:^(id ok, NSDictionary *err) {
                [GMTheme toast:[GMVal(ok, @"started") boolValue] ? @"Đang tìm bài mới…" : @"Đang quét rồi." in:self.view];
            }];
        } else {
            GMConfirm(@"Quét toàn bộ tài khoản?", @"Đọc lại mọi bài hát trong các chat, nhóm, kênh. Có thể mất vài phút.", @"Quét", ^{
                [GMCore call:@"sync.start" args:@{@"full": @YES} done:^(id ok, NSDictionary *err) {
                    [GMTheme toast:[GMVal(ok, @"started") boolValue] ? @"Đang quét toàn bộ…" : @"Đang quét rồi." in:self.view];
                }];
            });
        }
    } else if (ip.section == GMSecApp) {
        if (ip.row == 0) {
            [self.navigationController pushViewController:[[GMProxyViewController alloc] init] animated:YES];
        } else if (ip.row == 1) {
            GMChoose(self, @"DNS mã hoá (DoH / DoT)", @[@"Bật (nên dùng)", @"Tắt — dùng DNS của máy"], ^(NSInteger i) {
                [self setPref:@"secure_dns" value:@(i == 0)];
            });
        } else if (ip.row == 2) {
            [self.navigationController pushViewController:[[GMStorageViewController alloc] initWithStyle:UITableViewStyleGrouped]
                                                 animated:YES];
        } else {
            GMChoose(self, @"Giao diện", @[@"Sáng", @"Tối", @"Tự động (tối từ 19:00 tới 6:00)"], ^(NSInteger i) {
                [GMTheme setMode:i];
                [app reloadTheme];
            });
        }
    } else if (ip.section == GMSecLogout) {
        GMConfirm(@"Đăng xuất?", @"Bài đã tải về máy vẫn giữ. Đăng nhập lại để nghe tiếp nhạc từ Telegram.", @"Đăng xuất", ^{
            [GMCore call:@"auth.logout" args:nil done:nil];
        });
    }
}

@end

#pragma mark - Dữ liệu và bộ nhớ

@implementation GMStorageViewController {
    NSDictionary *_st;
}

static NSArray *GMKinds(void) {
    return @[@"offline", @"music", @"cover", @"avatar"];
}

static NSString *GMKindName(NSString *k) {
    if ([k isEqual:@"offline"]) return @"Bài đã tải về máy";
    if ([k isEqual:@"music"]) return @"Bài đã nghe (bộ nhớ đệm)";
    if ([k isEqual:@"cover"]) return @"Ảnh bìa";
    return @"Ảnh nguồn";
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Dữ liệu và bộ nhớ";
    [self load];
}

- (void)load {
    [GMCore call:@"cache.stats" args:nil done:^(id ok, NSDictionary *err) {
        if (err) {
            [GMTheme toast:GMErrorText(err) in:self.view];
            return;
        }
        _st = ok;
        [self.tableView reloadData];
    }];
}

- (NSInteger)numberOfSectionsInTableView:(UITableView *)tv {
    return 3;
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    return s == 0 ? GMKinds().count : (s == 1 ? 2 : 1);
}

- (NSString *)tableView:(UITableView *)tv titleForHeaderInSection:(NSInteger)s {
    if (s == 0 && _st) {
        NSNumber *free = GMVal(_st, @"free");
        return [NSString stringWithFormat:@"Đang dùng %@%@", [GMTheme sizeText:[GMVal(_st, @"total") doubleValue]],
                free ? [NSString stringWithFormat:@" · máy còn trống %@", [GMTheme sizeText:free.doubleValue]] : @""];
    }
    return nil;
}

- (NSString *)tableView:(UITableView *)tv titleForFooterInSection:(NSInteger)s {
    if (s == 0) return @"Chạm một dòng để xoá. Bài đã tải về máy không bao giờ tự xoá.";
    if (s == 1) return @"Bài nghe trọn được giữ để lần sau nghe không cần mạng. Đầy giới hạn thì tự xoá bài lâu không nghe nhất; máy sắp "
                       @"hết chỗ cũng tự dọn.";
    return nil;
}

static NSString *GMLimitText(NSInteger mb) {
    if (mb <= 0) return @"Không giới hạn";
    return mb >= 1024 ? [NSString stringWithFormat:@"%ld GB", (long)(mb / 1024)] : [NSString stringWithFormat:@"%ld MB", (long)mb];
}

static NSString *GMKeepText(NSInteger d) {
    if (d <= 0) return @"Mãi mãi";
    if (d == 3) return @"3 ngày";
    if (d == 7) return @"1 tuần";
    if (d == 30) return @"1 tháng";
    return [NSString stringWithFormat:@"%ld ngày", (long)d];
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    UITableViewCell *c = [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleValue1 reuseIdentifier:nil];
    if (ip.section == 0) {
        NSString *k = [GMKinds() objectAtIndex:ip.row];
        c.textLabel.text = GMKindName(k);
        c.detailTextLabel.text = [GMTheme sizeText:[GMVal(GMVal(_st, @"kinds"), k) doubleValue]];
    } else if (ip.section == 1) {
        if (ip.row == 0) {
            c.textLabel.text = @"Giới hạn bộ nhớ đệm";
            c.detailTextLabel.text = GMLimitText([GMVal(_st, @"limit_mb") integerValue]);
        } else {
            c.textLabel.text = @"Giữ bài đã nghe";
            c.detailTextLabel.text = GMKeepText([GMVal(_st, @"keep_days") integerValue]);
        }
        c.accessoryType = UITableViewCellAccessoryDisclosureIndicator;
    } else {
        c.textLabel.text = @"Xoá bộ nhớ đệm";
        c.textLabel.textColor = [UIColor colorWithRed:0.85 green:0.2 blue:0.2 alpha:1];
    }
    return c;
}

- (void)clear:(NSArray *)kinds title:(NSString *)title {
    GMConfirm(title, [kinds containsObject:@"offline"] ? @"Các bài này sẽ phải tải lại khi nghe." : nil, @"Xoá", ^{
        [GMCore call:@"cache.clear" args:@{@"kinds": kinds} done:^(id ok, NSDictionary *err) {
            if (!err) [GMTheme toast:[NSString stringWithFormat:@"Đã xoá %@", [GMTheme sizeText:[GMVal(ok, @"freed") doubleValue]]] in:self.view];
            [self load];
        }];
    });
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    if (ip.section == 0) {
        NSString *k = [GMKinds() objectAtIndex:ip.row];
        [self clear:@[k] title:[NSString stringWithFormat:@"Xoá «%@»?", GMKindName(k)]];
    } else if (ip.section == 1 && ip.row == 0) {
        NSArray *mb = @[@256, @512, @1024, @2048, @5120, @0];
        NSMutableArray *names = [NSMutableArray array];
        for (NSNumber *m in mb) [names addObject:GMLimitText(m.integerValue)];
        GMChoose(self, @"Giới hạn bộ nhớ đệm", names, ^(NSInteger i) {
            [GMCore call:@"cache.set" args:@{@"limit_mb": [mb objectAtIndex:i]} done:^(id ok, NSDictionary *err) { [self load]; }];
        });
    } else if (ip.section == 1) {
        NSArray *days = @[@3, @7, @30, @0];
        NSMutableArray *names = [NSMutableArray array];
        for (NSNumber *d in days) [names addObject:GMKeepText(d.integerValue)];
        GMChoose(self, @"Giữ bài đã nghe", names, ^(NSInteger i) {
            [GMCore call:@"cache.set" args:@{@"keep_days": [days objectAtIndex:i]} done:^(id ok, NSDictionary *err) { [self load]; }];
        });
    } else {
        [self clear:@[@"music", @"cover", @"avatar"] title:@"Xoá bộ nhớ đệm? (bài đã tải về máy giữ nguyên)"];
    }
}

@end

#import "GMSongActions.h"
#import "GMCore.h"
#import "GMPlayer.h"
#import "GMSongsViewController.h"
#import "GMTheme.h"

/// Các bảng đang mở (UIActionSheet / UIAlertView không giữ delegate).
static NSMutableSet *gOpen;

#pragma mark - Bảng thao tác dùng khối

@interface GMSheet : NSObject <UIActionSheetDelegate, UIAlertViewDelegate>
@property (nonatomic, strong) NSMutableArray *actions;  // khối theo thứ tự nút (không gồm Huỷ)
@property (nonatomic, copy) void (^textDone)(NSString *text);
@end

@implementation GMSheet

- (void)actionSheet:(UIActionSheet *)sheet clickedButtonAtIndex:(NSInteger)i {
    if (i >= 0 && i < (NSInteger)_actions.count) ((void (^)(void))[_actions objectAtIndex:i])();
}

- (void)actionSheet:(UIActionSheet *)sheet didDismissWithButtonIndex:(NSInteger)i {
    [gOpen removeObject:self];
}

- (void)alertView:(UIAlertView *)alert clickedButtonAtIndex:(NSInteger)i {
    if (i == 1 && _textDone) {
        NSString *t = [[alert textFieldAtIndex:0].text stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceCharacterSet]];
        if (t.length) _textDone(t);
    }
}

- (void)alertView:(UIAlertView *)alert didDismissWithButtonIndex:(NSInteger)i {
    [gOpen removeObject:self];
}

@end

/// Hỏi một dòng chữ (tên playlist…) — UIAlertView có ô nhập (iOS 5+).
void GMAskText(NSString *title, NSString *placeholder, NSString *initial, void (^done)(NSString *text)) {
    if (!gOpen) gOpen = [NSMutableSet set];
    GMSheet *h = [[GMSheet alloc] init];
    h.textDone = done;
    UIAlertView *a = [[UIAlertView alloc] initWithTitle:title message:nil delegate:h cancelButtonTitle:@"Huỷ" otherButtonTitles:@"Lưu", nil];
    a.alertViewStyle = UIAlertViewStylePlainTextInput;
    UITextField *f = [a textFieldAtIndex:0];
    f.placeholder = placeholder;
    f.text = initial;
    f.autocapitalizationType = UITextAutocapitalizationTypeSentences;
    [gOpen addObject:h];
    [a show];
}

#pragma mark - Chọn playlist

@interface GMPickPlaylistViewController : UITableViewController
@property (nonatomic, copy) NSArray *keys;
@end

@implementation GMPickPlaylistViewController {
    NSArray *_lists;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Thêm vào playlist";
    self.navigationItem.leftBarButtonItem =
        [[UIBarButtonItem alloc] initWithTitle:@"Huỷ" style:UIBarButtonItemStyleBordered target:self action:@selector(close)];
    [GMCore call:@"pl.list" args:nil done:^(id ok, NSDictionary *err) {
        _lists = GMVal(ok, @"lists") ?: @[];
        [self.tableView reloadData];
    }];
}

- (void)close {
    [self dismissViewControllerAnimated:YES completion:nil];
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    return _lists.count + 1;
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    UITableViewCell *c = [tv dequeueReusableCellWithIdentifier:@"p"] ?:
        [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleSubtitle reuseIdentifier:@"p"];
    if (ip.row == 0) {
        c.textLabel.text = @"+ Playlist mới…";
        c.textLabel.textColor = [GMTheme accent];
        c.detailTextLabel.text = nil;
    } else {
        NSDictionary *p = [_lists objectAtIndex:ip.row - 1];
        c.textLabel.text = GMVal(p, @"name");
        c.textLabel.textColor = [GMTheme text];
        c.detailTextLabel.text = [NSString stringWithFormat:@"%@ bài", GMVal(p, @"count")];
    }
    return c;
}

- (void)addTo:(id)plId name:(NSString *)name {
    UIView *host = self.presentingViewController.view;
    [GMCore call:@"pl.add" args:@{@"id": plId, @"keys": _keys} done:^(id ok, NSDictionary *err) {
        if (err) [GMTheme toast:GMErrorText(err) in:host];
        else [GMTheme toast:[NSString stringWithFormat:@"Đã thêm vào «%@»", name] in:host];
    }];
    [self close];
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    if (ip.row == 0) {
        GMAskText(@"Playlist mới", @"Tên playlist", nil, ^(NSString *name) {
            UIView *host = self.presentingViewController.view;
            [GMCore call:@"pl.create" args:@{@"name": name, @"keys": _keys} done:^(id ok, NSDictionary *err) {
                if (err) [GMTheme toast:GMErrorText(err) in:host];
                else [GMTheme toast:[NSString stringWithFormat:@"Đã tạo «%@»", name] in:host];
            }];
            [self close];
        });
        return;
    }
    NSDictionary *p = [_lists objectAtIndex:ip.row - 1];
    [self addTo:GMVal(p, @"id") name:GMVal(p, @"name")];
}

@end

#pragma mark - Bảng thao tác của bài

@implementation GMSongActions

+ (void)addKeys:(NSArray *)keys toPlaylistFrom:(UIViewController *)vc {
    if (!keys.count) return;
    GMPickPlaylistViewController *p = [[GMPickPlaylistViewController alloc] initWithStyle:UITableViewStylePlain];
    p.keys = keys;
    [vc presentViewController:[[UINavigationController alloc] initWithRootViewController:p] animated:YES completion:nil];
}

+ (void)showFor:(NSDictionary *)song in:(UIViewController *)vc from:(UIView *)anchor playlist:(id)playlistId
        changed:(void (^)(NSDictionary *))changed removed:(void (^)(void))removed {
    NSString *key = GMVal(song, @"key");
    if (!key) return;
    if (!gOpen) gOpen = [NSMutableSet set];
    GMSheet *h = [[GMSheet alloc] init];
    h.actions = [NSMutableArray array];
    UIActionSheet *sheet = [[UIActionSheet alloc] initWithTitle:GMVal(song, @"title") delegate:h cancelButtonTitle:nil
                                         destructiveButtonTitle:nil otherButtonTitles:nil];
    UIView *host = vc.view;
    void (^add)(NSString *, void (^)(void)) = ^(NSString *title, void (^blk)(void)) {
        [sheet addButtonWithTitle:title];
        [h.actions addObject:[blk copy]];
    };
    BOOL ok = [GMVal(song, @"ok") boolValue];
    if (ok) {
        add(@"Phát tiếp theo", ^{
            [[GMPlayer shared] insertNext:song];
            [GMTheme toast:@"Sẽ phát tiếp theo" in:host];
        });
        add(@"Thêm vào hàng chờ", ^{
            [[GMPlayer shared] append:song];
            [GMTheme toast:@"Đã thêm vào hàng chờ" in:host];
        });
    }
    BOOL fav = [GMVal(song, @"fav") boolValue];
    add(fav ? @"Bỏ khỏi Yêu thích" : @"Thêm vào Yêu thích", ^{
        [GMCore call:@"fav.set" args:@{@"key": key, @"on": @(!fav)} done:^(id r, NSDictionary *err) {
            if (err) {
                [GMTheme toast:GMErrorText(err) in:host];
                return;
            }
            NSMutableDictionary *s = [song mutableCopy];
            [s setObject:@(!fav) forKey:@"fav"];
            [[GMPlayer shared] updateSong:s];
            if (changed) changed(s);
            if (fav && removed && [playlistId isEqual:@"fav"]) removed();
        }];
    });
    add(@"Thêm vào playlist…", ^{ [self addKeys:@[key] toPlaylistFrom:vc]; });
    NSString *dl = GMVal(song, @"dl");
    BOOL mine = [dl isEqual:@"offline"] || [dl isEqual:@"queued"];
    add(mine ? @"Xoá bản đã tải" : @"Tải về máy (nghe không cần mạng)", ^{
        [GMCore call:mine ? @"dl.remove" : @"dl.add" args:@{@"keys": @[key]} done:^(id r, NSDictionary *err) {
            if (err) {
                [GMTheme toast:GMErrorText(err) in:host];
                return;
            }
            NSMutableDictionary *s = [song mutableCopy];
            if (mine) [s removeObjectForKey:@"dl"];
            else [s setObject:@"queued" forKey:@"dl"];
            [[GMPlayer shared] updateSong:s];
            if (changed) changed(s);
            if (mine && removed && [playlistId isEqual:@"offline"]) removed();
            [GMTheme toast:mine ? @"Đã xoá bản tải" : @"Đang tải về máy…" in:host];
        }];
    });
    NSString *src = GMVal(song, @"src");
    NSString *peer = GMVal(song, @"peer");
    if (src.length && peer.length && vc.navigationController) {
        add([NSString stringWithFormat:@"Mở nguồn: %@", src], ^{
            GMSongsViewController *s = [[GMSongsViewController alloc] initWithKind:GMSongsSource];
            s.source = peer;
            s.title = src;
            [vc.navigationController pushViewController:s animated:YES];
        });
    }
    BOOL realList = playlistId && ![playlistId isEqual:@"offline"];
    if (realList) {
        add([playlistId isEqual:@"recent"] ? @"Xoá lịch sử nghe" : @"Bỏ khỏi playlist này", ^{
            [GMCore call:@"pl.remove" args:@{@"id": playlistId, @"keys": @[key]} done:^(id r, NSDictionary *err) {
                if (err) return;
                if ([playlistId isEqual:@"fav"]) {
                    // Bỏ khỏi Yêu thích = bỏ thích: tim ở màn Đang phát cũng tắt
                    NSMutableDictionary *s = [song mutableCopy];
                    [s setObject:@NO forKey:@"fav"];
                    [[GMPlayer shared] updateSong:s];
                }
                if (removed) removed();
            }];
        });
        sheet.destructiveButtonIndex = (NSInteger)h.actions.count - 1;
    }
    sheet.cancelButtonIndex = [sheet addButtonWithTitle:@"Huỷ"];
    [gOpen addObject:h];
    if (UI_USER_INTERFACE_IDIOM() == UIUserInterfaceIdiomPad && anchor) {
        [sheet showFromRect:anchor.bounds inView:anchor animated:YES];
    } else {
        [sheet showInView:host.window ?: host];
    }
}

@end

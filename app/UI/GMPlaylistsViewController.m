#import "GMPlaylistsViewController.h"
#import "GMArt.h"
#import "GMCore.h"
#import "GMImages.h"
#import "GMSongActions.h"
#import "GMSongsViewController.h"
#import "GMTheme.h"

@implementation GMPlaylistsViewController {
    NSArray *_lists;
    NSInteger _fav, _recent;
}

- (void)viewDidLoad {
    [super viewDidLoad];
    self.title = @"Playlist";
    self.tableView.rowHeight = 52;
    self.navigationItem.rightBarButtonItem =
        [[UIBarButtonItem alloc] initWithTitle:@"Tạo mới" style:UIBarButtonItemStyleBordered target:self action:@selector(create)];
}

- (void)viewWillAppear:(BOOL)animated {
    [super viewWillAppear:animated];
    [self load];
}

- (void)load {
    [GMCore call:@"pl.list" args:nil done:^(id ok, NSDictionary *err) {
        if (err) return;
        _lists = GMVal(ok, @"lists") ?: @[];
        _fav = [GMVal(ok, @"fav") integerValue];
        _recent = [GMVal(ok, @"recent") integerValue];
        [self.tableView reloadData];
    }];
}

- (void)create {
    GMAskText(@"Playlist mới", @"Tên playlist", nil, ^(NSString *name) {
        [GMCore call:@"pl.create" args:@{@"name": name} done:^(id ok, NSDictionary *err) {
            if (err) [GMTheme toast:GMErrorText(err) in:self.view];
            [self load];
        }];
    });
}

- (NSInteger)numberOfSectionsInTableView:(UITableView *)tv {
    return 2;
}

- (NSString *)tableView:(UITableView *)tv titleForHeaderInSection:(NSInteger)s {
    return s == 1 && _lists.count ? @"Playlist của bạn" : nil;
}

- (NSInteger)tableView:(UITableView *)tv numberOfRowsInSection:(NSInteger)s {
    return s == 0 ? 2 : _lists.count;
}

- (UITableViewCell *)tableView:(UITableView *)tv cellForRowAtIndexPath:(NSIndexPath *)ip {
    UITableViewCell *c = [tv dequeueReusableCellWithIdentifier:@"pl"] ?:
        [[UITableViewCell alloc] initWithStyle:UITableViewCellStyleSubtitle reuseIdentifier:@"pl"];
    c.textLabel.font = [UIFont boldSystemFontOfSize:15];
    if (ip.section == 0) {
        BOOL fav = ip.row == 0;
        c.textLabel.text = fav ? @"Yêu thích" : @"Nghe gần đây";
        c.detailTextLabel.text = [NSString stringWithFormat:@"%ld bài", (long)(fav ? _fav : _recent)];
        c.imageView.image = [GMArt glyph:fav ? @"heartFill" : @"timer" size:32
                                  color:fav ? [UIColor colorWithRed:0.93 green:0.3 blue:0.4 alpha:1] : [GMTheme accent]];
        c.accessoryType = UITableViewCellAccessoryDisclosureIndicator;
        return c;
    }
    NSDictionary *p = [_lists objectAtIndex:ip.row];
    c.textLabel.text = GMVal(p, @"name");
    c.detailTextLabel.text = [NSString stringWithFormat:@"%@ bài", GMVal(p, @"count")];
    NSString *cover = GMVal(p, @"cover");
    c.imageView.image = cover ? [GMArt placeholderFor:cover size:32] : [GMArt glyph:@"queue" size:32 color:[GMTheme accent]];
    c.accessoryType = UITableViewCellAccessoryDetailDisclosureButton;
    return c;
}

- (void)tableView:(UITableView *)tv didSelectRowAtIndexPath:(NSIndexPath *)ip {
    [tv deselectRowAtIndexPath:ip animated:YES];
    GMSongsViewController *v = [[GMSongsViewController alloc] initWithKind:GMSongsPlaylist];
    if (ip.section == 0) {
        v.playlistId = ip.row == 0 ? @"fav" : @"recent";
        v.title = ip.row == 0 ? @"Yêu thích" : @"Nghe gần đây";
    } else {
        NSDictionary *p = [_lists objectAtIndex:ip.row];
        v.playlistId = GMVal(p, @"id");
        v.title = GMVal(p, @"name");
    }
    [self.navigationController pushViewController:v animated:YES];
}

/// Nút ⓘ: đổi tên playlist.
- (void)tableView:(UITableView *)tv accessoryButtonTappedForRowWithIndexPath:(NSIndexPath *)ip {
    if (ip.section != 1) return;
    NSDictionary *p = [_lists objectAtIndex:ip.row];
    GMAskText(@"Đổi tên playlist", @"Tên playlist", GMVal(p, @"name"), ^(NSString *name) {
        [GMCore call:@"pl.rename" args:@{@"id": GMVal(p, @"id"), @"name": name} done:^(id ok, NSDictionary *err) {
            [self load];
        }];
    });
}

- (BOOL)tableView:(UITableView *)tv canEditRowAtIndexPath:(NSIndexPath *)ip {
    return ip.section == 1;
}

- (NSString *)tableView:(UITableView *)tv titleForDeleteConfirmationButtonForRowAtIndexPath:(NSIndexPath *)ip {
    return @"Xoá";
}

- (void)tableView:(UITableView *)tv commitEditingStyle:(UITableViewCellEditingStyle)style forRowAtIndexPath:(NSIndexPath *)ip {
    if (style != UITableViewCellEditingStyleDelete) return;
    NSDictionary *p = [_lists objectAtIndex:ip.row];
    [GMCore call:@"pl.delete" args:@{@"id": GMVal(p, @"id")} done:^(id ok, NSDictionary *err) {
        [self load];
    }];
}

@end

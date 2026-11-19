#import "GMAlert.h"

@implementation GMBlockAlert
- (void)alertView:(UIAlertView *)a clickedButtonAtIndex:(NSInteger)i {
    if (self.done) self.done(i);
    self.done = nil;  // khối giữ hộp → bỏ để giải phóng
}
@end

GMBlockAlert *GMAlertShow(NSString *title, NSString *msg, NSString *cancel, NSString *other, void (^done)(NSInteger)) {
    GMBlockAlert *a = [[GMBlockAlert alloc] initWithTitle:title message:msg delegate:nil cancelButtonTitle:cancel otherButtonTitles:nil];
    if (other) [a addButtonWithTitle:other];
    a.delegate = a;
    a.done = done;
    [a show];
    return a;
}

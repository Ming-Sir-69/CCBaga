// Local update-check alert; receives non-secret display text and a report location. No browser/profile operations.
#import <AppKit/AppKit.h>
int main(int argc, char **argv) {
    @autoreleasepool {
        if (argc != 4) return 2;
        NSString *title = [NSString stringWithUTF8String:argv[1]];
        NSString *summary = [NSString stringWithUTF8String:argv[2]];
        NSString *path = [NSString stringWithUTF8String:argv[3]];
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
        [NSApp finishLaunching];
        NSAlert *alert = [[NSAlert alloc] init];
        alert.messageText = title;
        alert.informativeText = summary;
        alert.alertStyle = NSAlertStyleWarning;
        [alert addButtonWithTitle:@"知道了"];
        [alert addButtonWithTitle:@"查看报告"];
        [NSApp activateIgnoringOtherApps:YES];
        if ([alert runModal] == NSAlertSecondButtonReturn)
            [NSWorkspace.sharedWorkspace activateFileViewerSelectingURLs:@[[NSURL fileURLWithPath:path]]];
    }
    return 0;
}

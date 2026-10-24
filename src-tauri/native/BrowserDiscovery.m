// [INPUT]: NSWorkspace registrations, app Info.plist, standard Chromium roots and Local State profile names.
// [OUTPUT]: Read-only installed/default/history/profile metadata. No launch, migration, credentials or Safari data access.
// [POS]: Optional macOS discovery; paths and registrations are independent of import capability.
// Stable Brave/Chrome/Edge path entries adapted from browser2zen (MIT), commit
// 5bcc98ce226697def5d62d44806e209e60e47edd, src/extractors/{brave,chrome,edge}.py.
// Copyright (c) 2025 browser2zen contributors. See ../third-party/browser2zen-LICENSE.
#import <AppKit/AppKit.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#include <errno.h>

static NSArray<NSArray<NSString *> *> *discoveryCatalog(void) {
    // bundle ID, display name, application name, Application Support relative root
    return @[
        @[@"com.brave.Browser", @"Brave", @"Brave Browser.app", @"BraveSoftware/Brave-Browser"],
        @[@"com.brave.Browser.beta", @"Brave Beta", @"Brave Browser Beta.app", @"BraveSoftware/Brave-Browser-Beta"],
        @[@"com.brave.Browser.nightly", @"Brave Nightly", @"Brave Browser Nightly.app", @"BraveSoftware/Brave-Browser-Nightly"],
        @[@"com.google.Chrome", @"Google Chrome", @"Google Chrome.app", @"Google/Chrome"],
        @[@"com.google.Chrome.beta", @"Google Chrome Beta", @"Google Chrome Beta.app", @"Google/Chrome Beta"],
        @[@"com.google.Chrome.dev", @"Google Chrome Dev", @"Google Chrome Dev.app", @"Google/Chrome Dev"],
        @[@"com.google.Chrome.canary", @"Google Chrome Canary", @"Google Chrome Canary.app", @"Google/Chrome Canary"],
        @[@"com.google.chrome.for.testing", @"Google Chrome for Testing", @"Google Chrome for Testing.app", @"Google/Chrome for Testing"],
        @[@"com.microsoft.edgemac", @"Microsoft Edge", @"Microsoft Edge.app", @"Microsoft Edge"],
        @[@"com.microsoft.edgemac.Dev", @"Microsoft Edge Dev", @"Microsoft Edge Dev.app", @"Microsoft Edge Dev"],
        @[@"com.microsoft.edgemac.Beta", @"Microsoft Edge Beta", @"Microsoft Edge Beta.app", @"Microsoft Edge Beta"],
        @[@"com.microsoft.edgemac.Canary", @"Microsoft Edge Canary", @"Microsoft Edge Canary.app", @"Microsoft Edge Canary"],
        @[@"com.apple.Safari", @"Safari", @"Safari.app", @""],
    ];
}

static NSString *discoveryError(int code) {
    return code == EACCES || code == EPERM || code == ELOOP ? @"restricted" : code == ENOENT ? @"absent" : @"unknown";
}
static NSString *discoveryDirectory(NSString *path) {
    struct stat info;
    if (lstat(path.fileSystemRepresentation, &info)) return discoveryError(errno);
    if (S_ISLNK(info.st_mode)) return @"restricted";
    return S_ISDIR(info.st_mode) ? @"present" : @"unknown";
}
static BOOL discoveryLeaf(NSString *name) {
    return [name isKindOfClass:NSString.class] && name.length > 0 && name.length <= 160
        && ![name isEqualToString:@"."] && ![name isEqualToString:@".."]
        && [name rangeOfString:@"/"].location == NSNotFound
        && [name rangeOfString:@"\\"].location == NSNotFound
        && [name rangeOfCharacterFromSet:NSCharacterSet.controlCharacterSet].location == NSNotFound;
}
static BOOL discoveryProfileName(NSString *name) {
    if ([name isEqualToString:@"Default"]) return YES;
    if (![name hasPrefix:@"Profile "] || name.length <= 8) return NO;
    return [[name substringFromIndex:8] rangeOfCharacterFromSet:NSCharacterSet.decimalDigitCharacterSet.invertedSet].location == NSNotFound;
}
static NSDictionary *discoveryLocalState(NSString *root, NSString **state) {
    // The only browser data file this module opens. Reject links/non-files and bound memory.
    NSString *path = [root stringByAppendingPathComponent:@"Local State"];
    int fd = open(path.fileSystemRepresentation, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC);
    if (fd < 0) { *state = discoveryError(errno); return nil; }
    struct stat info;
    if (fstat(fd, &info) || !S_ISREG(info.st_mode) || info.st_size > 4 * 1024 * 1024) {
        close(fd); *state = @"unknown"; return nil;
    }
    NSMutableData *data = [NSMutableData data];
    char buffer[8192]; ssize_t size;
    while ((size = read(fd, buffer, sizeof(buffer))) > 0) {
        if (data.length + size > 4 * 1024 * 1024) { close(fd); *state = @"unknown"; return nil; }
        [data appendBytes:buffer length:(NSUInteger)size];
    }
    int error = errno; close(fd);
    if (size < 0) { *state = discoveryError(error); return nil; }
    id value = [NSJSONSerialization JSONObjectWithData:data options:0 error:NULL];
    if (![value isKindOfClass:NSDictionary.class]) { *state = @"unknown"; return nil; }
    id profile = value[@"profile"];
    id cache = [profile isKindOfClass:NSDictionary.class] ? profile[@"info_cache"] : nil;
    if (![cache isKindOfClass:NSDictionary.class]) { *state = @"unknown"; return nil; }
    *state = @"ready";
    return cache;
}
static NSDictionary *discoveryProfiles(NSString *root, BOOL enabled) {
    if (!enabled) return @{@"data_state": @"unknown", @"profile_state": @"disabled", @"profiles": @[]};
    NSString *dataState = discoveryDirectory(root);
    if (![dataState isEqualToString:@"present"])
        return @{@"data_state": dataState, @"profile_state": [dataState isEqualToString:@"absent"] ? @"empty" : dataState, @"profiles": @[]};
    NSError *error = nil;
    NSArray *children = [NSFileManager.defaultManager contentsOfDirectoryAtPath:root error:&error];
    if (!children) return @{@"data_state": @"present", @"profile_state": error.code == NSFileReadNoPermissionError ? @"restricted" : @"unknown", @"profiles": @[]};
    NSString *state = nil;
    NSDictionary *cache = discoveryLocalState(root, &state);
    if ([state isEqualToString:@"restricted"]) return @{@"data_state": @"present", @"profile_state": state, @"profiles": @[]};
    NSMutableSet *names = [NSMutableSet set];
    BOOL incomplete = children.count > 2048 || cache.count > 256;
    if (!incomplete) {
        for (NSString *name in children) if (discoveryProfileName(name)) [names addObject:name];
        for (NSString *name in cache) if (discoveryLeaf(name)) [names addObject:name]; else incomplete = YES;
    }
    NSMutableArray *profiles = [NSMutableArray array];
    for (NSString *name in [[names allObjects] sortedArrayUsingSelector:@selector(compare:)]) {
        NSString *presence = discoveryDirectory([root stringByAppendingPathComponent:name]);
        if ([presence isEqualToString:@"absent"]) continue; // Stale cache entry is not a live profile.
        if (![presence isEqualToString:@"present"]) { incomplete = YES; continue; }
        id entry = cache[name], label = [entry isKindOfClass:NSDictionary.class] ? entry[@"name"] : nil;
        if (![label isKindOfClass:NSString.class] || ![label length] || [label length] > 160
            || [label rangeOfCharacterFromSet:NSCharacterSet.controlCharacterSet].location != NSNotFound) label = name;
        [profiles addObject:@{@"id": name, @"name": label}];
    }
    // Malformed/incomplete metadata must remain unknown even when directory-only fallback is available.
    NSString *profileState = incomplete || [state isEqualToString:@"unknown"] ? @"unknown" : profiles.count ? @"ready" : @"empty";
    return @{@"data_state": @"present", @"profile_state": profileState, @"profiles": profiles};
}
static NSString *discoveryCanonical(NSURL *url) { return url.URLByResolvingSymlinksInPath.standardizedURL.path; }
static NSDictionary *discoveryApp(NSURL *url) {
    NSString *path = discoveryCanonical(url);
    NSBundle *bundle = [NSBundle bundleWithPath:path];
    NSDictionary *info = bundle.infoDictionary;
    NSString *identifier = bundle.bundleIdentifier, *exe = info[@"CFBundleExecutable"];
    if (!identifier.length || ![info[@"CFBundlePackageType"] isEqual:@"APPL"] || !discoveryLeaf(exe)
        || ![NSFileManager.defaultManager isExecutableFileAtPath:bundle.executablePath ?: @""]) return nil;
    id version = info[@"CFBundleShortVersionString"] ?: info[@"CFBundleVersion"];
    if (![version isKindOfClass:NSString.class]) version = NSNull.null;
    id name = info[@"CFBundleDisplayName"] ?: info[@"CFBundleName"] ?: path.lastPathComponent;
    if (![name isKindOfClass:NSString.class]) name = identifier;
    return @{@"bundle_id": identifier, @"name": name, @"path": path, @"version": version};
}
// Dependency injection is limited to this internal function for fixture tests, never exposed to the UI.
static NSDictionary *discoverBrowsers(NSString *home, NSArray<NSString *> *appRoots, NSArray<NSURL *> *registered, NSURL *defaultURL, BOOL profilesEnabled) {
    NSMutableArray *urls = [registered mutableCopy];
    if (defaultURL) [urls addObject:defaultURL];
    for (NSArray *spec in discoveryCatalog()) for (NSString *root in appRoots)
        [urls addObject:[NSURL fileURLWithPath:[root stringByAppendingPathComponent:spec[2]]]];
    NSMutableDictionary<NSString *, NSMutableArray *> *apps = [NSMutableDictionary dictionary];
    NSMutableSet *seen = [NSMutableSet set];
    for (NSURL *url in urls) {
        NSString *path = discoveryCanonical(url);
        if ([seen containsObject:path]) continue;
        [seen addObject:path];
        NSDictionary *app = discoveryApp(url);
        if (!app) continue;
        NSString *identifier = app[@"bundle_id"];
        if (!apps[identifier]) apps[identifier] = [NSMutableArray array];
        [apps[identifier] addObject:app];
    }
    NSString *defaultPath = defaultURL ? discoveryCanonical(defaultURL) : nil;
    NSMutableArray *items = [NSMutableArray array];
    NSMutableSet *known = [NSMutableSet set];
    for (NSArray *spec in discoveryCatalog()) {
        NSString *identifier = spec[0], *relative = spec[3];
        [known addObject:identifier];
        NSString *root = relative.length ? [[home stringByAppendingPathComponent:@"Library/Application Support"] stringByAppendingPathComponent:relative] : nil;
        NSDictionary *profiles = root ? discoveryProfiles(root, profilesEnabled) : @{@"data_state": @"unknown", @"profile_state": @"unsupported", @"profiles": @[]};
        NSArray *installations = apps[identifier] ?: @[];
        // Include historical data, inaccessible roots and known absent channels explicitly.
        BOOL isDefault = NO;
        for (NSDictionary *app in installations) if ([app[@"path"] isEqual:defaultPath]) isDefault = YES;
        NSMutableDictionary *item = [profiles mutableCopy];
        [item addEntriesFromDictionary:@{@"id":identifier, @"name":spec[1], @"bundle_id":identifier,
            @"installed": installations.count ? @YES : @NO, @"is_default":isDefault ? @YES : @NO,
            @"installations":installations, @"data_root":root ?: (id)NSNull.null,
            @"import_capability":@"unavailable", @"kind":relative.length ? @"chromium" : @"safari"}];
        [items addObject:item];
    }
    // Unknown registered HTTPS handlers can be shown, but their profiles/import remain unknown.
    for (NSString *identifier in apps) if (![known containsObject:identifier]) {
        NSArray *installations = apps[identifier]; BOOL isDefault = NO;
        for (NSDictionary *app in installations) if ([app[@"path"] isEqual:defaultPath]) isDefault = YES;
        [items addObject:@{@"id":identifier,@"bundle_id":identifier,@"name":installations[0][@"name"],
            @"installed":@YES,@"is_default":isDefault ? @YES : @NO,@"installations":installations,
            @"kind":@"other",@"data_root":NSNull.null,@"data_state":@"unknown",@"profile_state":@"unsupported",
            @"profiles":@[],@"import_capability":@"unavailable"}];
    }
    [items sortUsingComparator:^NSComparisonResult(NSDictionary *a, NSDictionary *b) {
        if ([a[@"is_default"] boolValue] != [b[@"is_default"] boolValue]) return [a[@"is_default"] boolValue] ? NSOrderedAscending : NSOrderedDescending;
        if ([a[@"installed"] boolValue] != [b[@"installed"] boolValue]) return [a[@"installed"] boolValue] ? NSOrderedAscending : NSOrderedDescending;
        return [a[@"name"] compare:b[@"name"]];
    }];
    BOOL defaultKnown = NO;
    for (NSDictionary *item in items) if ([item[@"is_default"] boolValue]) defaultKnown = YES;
    return @{@"supported":@YES,@"default_known":defaultKnown ? @YES : @NO,@"items":items};
}
char *rf_browser_discover(bool profilesEnabled) {
    @autoreleasepool {
        NSWorkspace *workspace = NSWorkspace.sharedWorkspace;
        NSURL *web = [NSURL URLWithString:@"https://example.com"];
        NSMutableArray *urls = [[workspace URLsForApplicationsToOpenURL:web] mutableCopy] ?: [NSMutableArray array];
        for (NSArray *spec in discoveryCatalog()) {
            NSURL *url = [workspace URLForApplicationWithBundleIdentifier:spec[0]];
            if (url) [urls addObject:url];
        }
        NSDictionary *result = discoverBrowsers(NSHomeDirectory(), @[@"/Applications", [NSHomeDirectory() stringByAppendingPathComponent:@"Applications"], @"/System/Applications"], urls, [workspace URLForApplicationToOpenURL:web], profilesEnabled);
        NSData *data = [NSJSONSerialization dataWithJSONObject:result options:0 error:NULL];
        return strdup([[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding].UTF8String);
    }
}

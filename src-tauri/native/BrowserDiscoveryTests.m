// Fixture-only discovery checks. No real home, registrations, browser launches or credential APIs.
#import "BrowserDiscovery.m"
#include <assert.h>
static int assertions = 0;
#define CHECK(x) do { assertions++; assert((x)); } while (0)
static void mkdirs(NSString *path) { assert([NSFileManager.defaultManager createDirectoryAtPath:path withIntermediateDirectories:YES attributes:nil error:NULL]); }
static void writeJSON(NSString *path, id value) { mkdirs(path.stringByDeletingLastPathComponent); assert([[NSJSONSerialization dataWithJSONObject:value options:0 error:NULL] writeToFile:path atomically:YES]); }
static NSURL *fakeApp(NSString *apps, NSString *filename, NSString *identifier, NSString *version) {
    NSString *path = [apps stringByAppendingPathComponent:filename];
    mkdirs([path stringByAppendingPathComponent:@"Contents/MacOS"]);
    NSDictionary *info = @{@"CFBundleIdentifier":identifier,@"CFBundleName":filename,@"CFBundlePackageType":@"APPL",@"CFBundleExecutable":@"fixture",@"CFBundleShortVersionString":version};
    assert([info writeToFile:[path stringByAppendingPathComponent:@"Contents/Info.plist"] atomically:YES]);
    NSString *exe = [path stringByAppendingPathComponent:@"Contents/MacOS/fixture"];
    assert([@"fixture only; never executed" writeToFile:exe atomically:YES encoding:NSUTF8StringEncoding error:NULL]); chmod(exe.fileSystemRepresentation,0700);
    return [NSURL fileURLWithPath:path];
}
static NSDictionary *item(NSDictionary *result, NSString *identifier) {
    for (NSDictionary *entry in result[@"items"]) if ([entry[@"id"] isEqual:identifier]) return entry;
    return nil;
}
int main(int argc, char **argv) { @autoreleasepool {
    assert(argc == 2);
    NSString *base = [NSString stringWithUTF8String:argv[1]];
    assert([base containsString:@"/_trash/"]);
    NSString *home = [base stringByAppendingPathComponent:NSUUID.UUID.UUIDString];
    NSString *apps = [home stringByAppendingPathComponent:@"Applications"];
    mkdirs(apps);
    NSURL *brave = fakeApp(apps,@"Brave Browser.app",@"com.brave.Browser",@"155.1.97.59");
    fakeApp(apps,@"Microsoft Edge Dev.app",@"com.microsoft.edgemac.Dev",@"157.0.4334.0");
    fakeApp(apps,@"Safari.app",@"com.apple.Safari",@"27.0.1");
    NSString *root = [home stringByAppendingPathComponent:@"Library/Application Support/Google/Chrome"];
    mkdirs([root stringByAppendingPathComponent:@"Default"]); mkdirs([root stringByAppendingPathComponent:@"Profile 1"]);
    NSString *local = [root stringByAppendingPathComponent:@"Local State"];
    NSDictionary *metadata = @{@"profile":@{@"info_cache":@{@"Default":@{@"name":@"Synthetic A",@"user_name":@"not-exposed"},@"Profile 1":@{@"name":@"Synthetic B"},@"Profile 99":@{@"name":@"stale"}}},@"token":@"secret-sentinel-not-exposed"};
    writeJSON(local,metadata);
    // Unreadable secret files must never participate in discovery.
    for (NSString *name in @[@"Cookies",@"Login Data"]) { NSString *p=[[root stringByAppendingPathComponent:@"Default"] stringByAppendingPathComponent:name]; [@"synthetic secret" writeToFile:p atomically:YES encoding:NSUTF8StringEncoding error:NULL]; chmod(p.fileSystemRepresentation,0000); }
    NSData *before = [NSData dataWithContentsOfFile:local];
    NSDictionary *result = discoverBrowsers(home,@[apps],@[brave,brave],brave,YES);
    CHECK([result[@"default_known"] boolValue]);
    CHECK([result[@"items"][0][@"id"] isEqual:@"com.brave.Browser"]);
    NSDictionary *chrome = item(result,@"com.google.Chrome");
    CHECK(![chrome[@"installed"] boolValue]); CHECK([chrome[@"data_state"] isEqual:@"present"]);
    CHECK([chrome[@"profiles"] count] == 2); CHECK([chrome[@"profiles"][1][@"name"] isEqual:@"Synthetic B"]);
    CHECK([chrome[@"import_capability"] isEqual:@"unavailable"]);
    CHECK([item(result,@"com.brave.Browser")[@"installations"] count] == 1);
    CHECK([item(result,@"com.brave.Browser")[@"profile_state"] isEqual:@"empty"]);
    CHECK([item(result,@"com.microsoft.edgemac.Dev")[@"installations"][0][@"version"] isEqual:@"157.0.4334.0"]);
    CHECK([item(result,@"com.apple.Safari")[@"profile_state"] isEqual:@"unsupported"]);
    NSString *json = [[NSString alloc] initWithData:[NSJSONSerialization dataWithJSONObject:result options:0 error:NULL] encoding:NSUTF8StringEncoding];
    CHECK(![json containsString:@"not-exposed"]); CHECK(![json containsString:@"Cookies"]); CHECK(![json containsString:@"Login Data"]);
    CHECK([before isEqualToData:[NSData dataWithContentsOfFile:local]]); // No metadata rewriting.
    NSDictionary *disabled=discoverBrowsers(home,@[apps],@[],brave,NO);
    CHECK([item(disabled,@"com.google.Chrome")[@"profile_state"] isEqual:@"disabled"]);
    CHECK([item(disabled,@"com.google.Chrome")[@"profiles"] count] == 0);
    chmod(local.fileSystemRepresentation,0000);
    CHECK([discoveryProfiles(root,YES)[@"profile_state"] isEqual:@"restricted"]);
    chmod(local.fileSystemRepresentation,0600);
    [NSFileManager.defaultManager removeItemAtPath:local error:NULL];
    CHECK([discoveryProfiles(root,YES)[@"profiles"] count] == 2); // Directory-only fallback.
    [@"broken JSON" writeToFile:local atomically:YES encoding:NSUTF8StringEncoding error:NULL];
    CHECK([discoveryProfiles(root,YES)[@"profile_state"] isEqual:@"unknown"]);
    [NSFileManager.defaultManager removeItemAtPath:local error:NULL];
    assert(symlink("Default/Cookies",local.fileSystemRepresentation)==0);
    CHECK([discoveryProfiles(root,YES)[@"profile_state"] isEqual:@"restricted"]);
    [NSFileManager.defaultManager removeItemAtPath:local error:NULL];
    writeJSON(local,@{@"profile":@{@"info_cache":@{@"../escape":@{@"name":@"unsafe"}}}});
    CHECK([discoveryProfiles(root,YES)[@"profile_state"] isEqual:@"unknown"]);
    int fd=open(local.fileSystemRepresentation,O_WRONLY);assert(fd>=0);assert(ftruncate(fd,5*1024*1024)==0);close(fd);
    CHECK([discoveryProfiles(root,YES)[@"profile_state"] isEqual:@"unknown"]);
    NSURL *other=fakeApp(apps,@"Other.app",@"fixture.other",@"1");
    NSDictionary *unknown=discoverBrowsers(home,@[apps],@[other,[NSURL fileURLWithPath:@"/missing/app"]],other,YES);
    CHECK([unknown[@"items"][0][@"id"] isEqual:@"fixture.other"]);
    CHECK([unknown[@"items"][0][@"profile_state"] isEqual:@"unsupported"]);
    CHECK(![discoverBrowsers(home,@[apps],@[],nil,YES)[@"default_known"] boolValue]);
    CHECK(![discoverBrowsers(home,@[apps],@[],[NSURL fileURLWithPath:@"/missing/app"],YES)[@"default_known"] boolValue]);
    CHECK([[NSData dataWithContentsOfFile:local] length]==5*1024*1024);
    assert([NSFileManager.defaultManager removeItemAtPath:home error:NULL]);
    printf("Browser discovery: %d assertions passed; fixtures only\n", assertions);
} return 0; }

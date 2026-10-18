'use strict';

// Compile and exercise pure native parsers. The production entry point is
// deliberately excluded: no browser launch or URL registration is performed.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

test('native archive and private-pipe parsers accept navigation only', { skip: process.platform !== 'darwin' }, () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'pf-archive-parser-'));
  try {
    const original = fs.readFileSync(path.join(__dirname, '../scripts/companion-launcher.swift'), 'utf8');
    const marker = '// Native launcher entry point.';
    assert.equal(original.split(marker).length, 2);
    const driver = String.raw`
let id="ABCDEF01-2345-6789-ABCD-EF0123456789"
assert(nativeLaunchRequest("photofindry-local://open") == .workspace)
assert(nativeLaunchRequest("photofindry://archive/v1/"+id) == .archive(id.lowercased()))
for invalid in ["photofindry-local://open?archive="+id,"photofindry-local://open/","photofindry://archive/v1/"+id+"?attach=1","photofindry://archive/v1/"+id+"#data","photofindry://archive/v1/"+id+"/extra","photofindry://user@archive/v1/"+id,"photofindry://archive:80/v1/"+id,"photofindry://archive/v1/%61bcdef01-2345-6789-abcd-ef0123456789","photofindry://archive/v1/../../path","photofindry://archive/v2/"+id,"PHOTOFINDRY://archive/v1/"+id] { assert(nativeLaunchRequest(invalid)==nil) }
let ticket=String(repeating:"a",count:64)
let plain=try! JSONSerialization.data(withJSONObject:["port":23456,"ticket":ticket])
assert(localWorkspaceURL(plain)?.host=="127.0.0.1")
assert(localWorkspaceURL(plain)?.fragment=="bootstrap="+ticket)
let navigation=try! JSONSerialization.data(withJSONObject:["port":23456,"ticket":ticket,"archiveId":id])
assert(localWorkspaceURL(navigation)?.fragment=="bootstrap="+ticket+"&archive="+id.lowercased())
for invalid in ["{\"port\":23456,\"ticket\":\""+ticket+"\",\"archiveId\":\"/private/path\"}","{\"port\":true,\"ticket\":\""+ticket+"\"}","{\"port\":\"23456\",\"ticket\":\""+ticket+"\"}","{\"port\":23456.0,\"ticket\":\""+ticket+"\"}","{\"port\":23456,\"ticket\":\""+ticket+"\",\"ticket\":\""+ticket+"\"}"] {assert(localWorkspaceURL(Data(invalid.utf8))==nil)}
let appRoot="/private/tmp/PhotoFindry.app"
assert(enclosingBrowserApplication(appRoot+"/Contents/Resources/companion/bin/CompanionLauncher")==appRoot)
assert(enclosingBrowserApplication(appRoot+"/Contents/Resources/companion/bin/CompanionLauncher.app/Contents/MacOS/CompanionLauncher")==appRoot)
for invalid in ["/private/tmp/Other/bin/CompanionLauncher",appRoot+"/Contents/Resources/other/bin/CompanionLauncher",appRoot+"/Contents/Resources/companion/bin/Other","/private/tmp/Plain/Contents/Resources/companion/bin/CompanionLauncher"] {assert(enclosingBrowserApplication(invalid)==nil)}
let application=URL(fileURLWithPath:"/private/tmp/photofindry-parser-fixture/bin/CompanionLauncher.app")
var selected=[String:URL](),requests=[String]()
assert(selectLocalLinkHandlers(application,timeout:0.25,setHandler:{ url,scheme,completion in
    assert(url==application);requests.append(scheme)
    DispatchQueue.main.async { selected[scheme]=url;completion(nil) }
},resolve:{selected[$0.scheme!]}))
assert(requests==["photofindry-local","photofindry"])
var calls=0
assert(!selectLocalLinkHandlers(application,timeout:0.02,setHandler:{_,_,completion in
    calls+=1;completion(NSError(domain:"fixture-denied",code:1))
},resolve:{_ in assertionFailure("Denied registration must not be reported successful");return application}))
assert(calls==1)
let older=URL(fileURLWithPath:"/private/tmp/older-photofindry/bin/CompanionLauncher.app")
assert(!selectLocalLinkHandlers(application,timeout:0.02,setHandler:{_,_,completion in completion(nil)},resolve:{_ in older}))
assert(!selectLocalLinkHandlers(application,timeout:0.02,setHandler:{_,_,_ in },resolve:{_ in application}))
var lookups=0
assert(selectLocalLinkHandlers(application,timeout:0.25,setHandler:{_,_,completion in completion(nil)},resolve:{_ in
    lookups+=1;return lookups==1 ? older : application
}))
assert(lookups>=3)
print("Native launch parsers passed; no browser or registration was invoked.")
`;
    const source = path.join(root, 'main.swift'), binary = path.join(root, 'parser');
    fs.writeFileSync(source, original.slice(0, original.indexOf(marker)) + driver);
    const compiled = spawnSync('/usr/bin/xcrun', ['swiftc', '-framework', 'AppKit', '-framework', 'CoreServices', source, '-o', binary], { encoding: 'utf8', timeout: 30000 });
    assert.equal(compiled.status, 0, compiled.stderr);
    const result = spawnSync(binary, [fs.realpathSync(root)], { encoding: 'utf8', timeout: 5000 });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.trim(), 'Native launch parsers passed; no browser or registration was invoked.');
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('native uninstall requires its own package, explicit confirmation and a revalidated Trash target', { skip: process.platform !== 'darwin' }, () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'pf-uninstall-parser-'));
  try {
    const original = fs.readFileSync(path.join(__dirname, '../scripts/companion-launcher.swift'), 'utf8');
    const marker = '// Native launcher entry point.';
    assert.equal(original.split(marker).length, 2);
    const driver = String.raw`
// This directory actually exists under canonical /private/var on macOS.
// Foundation's standardization rewrites it to /var; lexical validation must not.
assert(CommandLine.arguments.count==2)
assert(uninstallPath(CommandLine.arguments[1]))
assert(nativeHelperMode(["--confirm-uninstall"]) == .confirmUninstall)
assert(nativeHelperMode(["--trash-package"]) == .trashPackage)
assert(nativeHelperMode(["--uninstall-error"]) == .uninstallError)
for arguments in [[],["--trash-package","/tmp/path"],["--confirm-uninstall","--yes"],["--trash-package","--trash-package"],["--open-local","--trash-package"],["--uninstall"],["photofindry-local://uninstall"]] {assert(nativeHelperMode(arguments)==nil)}
let package="/private/tmp/PhotoFindry",library="/private/tmp/Preserved Library",owner:UInt32=501
let helper=package+"/bin/CompanionLauncher"
func input(_ fields:[String:Any])->Data {try! JSONSerialization.data(withJSONObject:fields,options:[.sortedKeys])}
let notice=input(["packagePath":package,"libraryPath":library,"message":"Another PhotoFindry process is using this package."])
assert(nativeUninstallErrorRequest(notice)?.request.libraryPath==library)
assert(nativeUninstallErrorRequest(notice)?.message=="Another PhotoFindry process is using this package.")
for invalid in ["",String(repeating:"x",count:2049),"broken\nline","nul\u{0}"] {
    assert(nativeUninstallErrorRequest(input(["packagePath":package,"libraryPath":library,"message":invalid]))==nil)
}
assert(nativeUninstallErrorRequest(input(["packagePath":package,"libraryPath":library,"message":"error","interactive":true]))==nil)
assert(nativeUninstallErrorRequest(Data(#"{"packagePath":"/tmp/PF","libraryPath":"/tmp/Library","message":"first","message":"second"}"#.utf8))==nil)
let confirm=input(["packagePath":package,"libraryPath":library])
let trash=input(["packagePath":package,"libraryPath":library,"interactive":false])
assert(nativeUninstallRequest(confirm,confirmation:true)==NativeUninstallRequest(packagePath:package,libraryPath:library,interactive:false))
assert(nativeUninstallRequest(trash,confirmation:false)==NativeUninstallRequest(packagePath:package,libraryPath:library,interactive:false))
assert(nativeUninstallRequest(input(["packagePath":package,"libraryPath":library,"interactive":true]),confirmation:false)?.interactive==true)
let unicode=input(["packagePath":"/Users/Zoë/PhotoFindry","libraryPath":"/Users/Zoë/Библиотека"])
assert(nativeUninstallRequest(unicode,confirmation:true)?.libraryPath=="/Users/Zoë/Библиотека")
let escaped=Data(#"{"packagePath":"\/Users\/Zo\u00eb\/PhotoFindry","libraryPath":"\/Users\/Zo\u00eb\/Library"}"#.utf8)
assert(nativeUninstallRequest(escaped,confirmation:true)?.packagePath=="/Users/Zoë/PhotoFindry")
for bad in ["relative","/","/tmp/../PhotoFindry","/tmp/./PhotoFindry","/tmp//PhotoFindry","/tmp/PhotoFindry/","/tmp/line\nbreak","/tmp/nul\u{0}","/"+String(repeating:"a",count:4096)] {
    assert(nativeUninstallRequest(input(["packagePath":bad,"libraryPath":library]),confirmation:true)==nil)
    assert(nativeUninstallRequest(input(["packagePath":package,"libraryPath":bad]),confirmation:true)==nil)
}
for bad in [
    "{}", "[]", "{\"packagePath\":\"/tmp/PhotoFindry\"}",
    "{\"packagePath\":\"/tmp/PhotoFindry\",\"libraryPath\":\"/tmp/Library\",\"packagePath\":\"/tmp/Other\"}",
    "{\"packagePath\":\"/tmp/PhotoFindry\",\"libraryPath\":\"/tmp/Library\",\"unknown\":false}",
    "{\"packagePath\":false,\"libraryPath\":\"/tmp/Library\"}",
    "{\"packagePath\":\"/tmp/PhotoFindry\",\"libraryPath\":true}",
    "{\"packagePath\":\"/tmp/PhotoFindry\",\"libraryPath\":\"/tmp/Library\"}{}"
] {assert(nativeUninstallRequest(Data(bad.utf8),confirmation:true)==nil)}
assert(nativeUninstallRequest(trash,confirmation:true)==nil)
assert(nativeUninstallRequest(confirm,confirmation:false)==nil)
for bad:Any in ["true",1,NSNull(),[true]] {
    assert(nativeUninstallRequest(input(["packagePath":package,"libraryPath":library,"interactive":bad]),confirmation:false)==nil)
}
assert(nativeUninstallRequest(Data(repeating:32,count:32769),confirmation:false)==nil)
assert(nativeLaunchRequest("photofindry-local://uninstall")==nil)
assert(nativeLaunchRequest("photofindry-local://open?uninstall=1")==nil)
var files=[String:NativeUninstallIdentity](),inode:UInt64=10
func identity(_ directory:Bool,owner:UInt32=501,inode:UInt64=99)->NativeUninstallIdentity {
    NativeUninstallIdentity(device:1,inode:inode,mode:UInt32(directory ? S_IFDIR : S_IFREG)|0o700,owner:owner,size:100,modified:10)
}
for directory in ["/private","/private/tmp",package,package+"/bin",package+"/bin/CompanionLauncher.app",package+"/bin/CompanionLauncher.app/Contents",package+"/bin/CompanionLauncher.app/Contents/MacOS",library] {
    inode+=1;files[directory]=identity(true,inode:inode)
}
for executable in [helper,package+"/bin/photofindry",package+"/bin/CompanionLauncher.app/Contents/MacOS/CompanionLauncher"] {
    inode+=1;files[executable]=identity(false,inode:inode)
}
let request=nativeUninstallRequest(confirm,confirmation:true)!
assert(uninstallPackageIdentities(request,executingPath:helper,owner:owner,inspect:{files[$0]}) != nil)
assert(uninstallPackageIdentities(request,executingPath:package+"/bin/CompanionLauncher.app/Contents/MacOS/CompanionLauncher",owner:owner,inspect:{files[$0]}) != nil)
assert(uninstallPackageIdentities(request,executingPath:"/private/tmp/Other/bin/CompanionLauncher",owner:owner,inspect:{files[$0]}) == nil)
var prompts=0,moves=0
assert(confirmNativeUninstall(confirm,executingPath:helper,owner:owner,inspect:{files[$0]},present:{ value in
    assert(value.packagePath==package && value.libraryPath==library);prompts+=1;return false
})==false)
assert(confirmNativeUninstall(confirm,executingPath:helper,owner:owner,inspect:{files[$0]},present:{_ in prompts+=1;return true})==true)
assert(prompts==2 && moves==0)
let destination=URL(fileURLWithPath:"/Users/test/.Trash/PhotoFindry")
assert(trashNativePackage(trash,executingPath:helper,owner:owner,inspect:{files[$0]},moveToTrash:{target in
    assert(target.path==package);moves+=1;return destination
})==destination)
assert(moves==1)
// The native strategy cannot redirect removal to another package or a library.
for badLibrary in [package,package+"/Library","/private/tmp"] {
    let unsafe=input(["packagePath":package,"libraryPath":badLibrary])
    assert(confirmNativeUninstall(unsafe,executingPath:helper,owner:owner,inspect:{files[$0]},present:{_ in assertionFailure();return true})==nil)
}
let fixtures=files
for badPath in [package,package+"/bin",helper,package+"/bin/photofindry",package+"/bin/CompanionLauncher.app","/private/tmp",library] {
    files=fixtures
    files[badPath]=NativeUninstallIdentity(device:1,inode:888,mode:UInt32(S_IFLNK)|0o777,owner:owner,size:20,modified:10)
    assert(trashNativePackage(trash,executingPath:helper,owner:owner,inspect:{files[$0]},moveToTrash:{_ in assertionFailure();return destination})==nil)
}
files=fixtures
// A differently cased alias of the package is still the same physical folder.
let alias="/private/tmp/photofindry"
files[alias]=files[package]
files[alias+"/data"]=identity(true,inode:12345)
let overlap=input(["packagePath":package,"libraryPath":alias+"/data","interactive":false])
assert(trashNativePackage(overlap,executingPath:helper,owner:owner,inspect:{files[$0]},moveToTrash:{_ in assertionFailure();return destination})==nil)
files=fixtures
files[package]=identity(true,owner:502)
assert(uninstallPackageIdentities(request,executingPath:helper,owner:owner,inspect:{files[$0]})==nil)
files=fixtures
files[package+"/catalog.sqlite"]=identity(false)
assert(uninstallPackageIdentities(request,executingPath:helper,owner:owner,inspect:{files[$0]})==nil)
files=fixtures
files.removeValue(forKey:package+"/bin/CompanionLauncher.app/Contents/MacOS/CompanionLauncher")
assert(uninstallPackageIdentities(request,executingPath:helper,owner:owner,inspect:{files[$0]})==nil)
files=fixtures
var checks=0
assert(trashNativePackage(trash,executingPath:helper,owner:owner,inspect:{ path in
    if path==package+"/bin/photofindry" {checks+=1;if checks>1 {return identity(false,inode:1234)}}
    return files[path]
},moveToTrash:{_ in assertionFailure();return destination})==nil)
assert(trashNativePackage(trash,executingPath:helper,owner:owner,inspect:{files[$0]},moveToTrash:{_ in nil})==nil)
assert(moves==1 && prompts==2)
files=fixtures
let browserRoot="/private/tmp/PhotoFindry.app",embedded=browserRoot+"/Contents/Resources/companion"
let browserHelper=embedded+"/bin/CompanionLauncher"
for directory in [browserRoot,browserRoot+"/Contents",browserRoot+"/Contents/MacOS",browserRoot+"/Contents/Resources",embedded,embedded+"/bin",embedded+"/bin/CompanionLauncher.app",embedded+"/bin/CompanionLauncher.app/Contents",embedded+"/bin/CompanionLauncher.app/Contents/MacOS"] {inode+=1;files[directory]=identity(true,inode:inode)}
for file in [browserRoot+"/Contents/MacOS/PhotoFindry",browserRoot+"/Contents/Info.plist",browserRoot+"/Contents/Resources/browser-host-manifest.json",browserHelper,embedded+"/bin/photofindry",embedded+"/bin/CompanionLauncher.app/Contents/MacOS/CompanionLauncher"] {inode+=1;files[file]=identity(false,inode:inode)}
let browserRequest=NativeUninstallRequest(packagePath:browserRoot,libraryPath:library,interactive:false)
assert(uninstallPackageIdentities(browserRequest,executingPath:browserHelper,owner:owner,inspect:{files[$0]}) != nil)
let browserInput=input(["packagePath":browserRoot,"libraryPath":library,"interactive":false])
assert(trashNativePackage(browserInput,executingPath:browserHelper,owner:owner,inspect:{files[$0]},moveToTrash:{target in assert(target.path==browserRoot);return destination})==destination)
let innerRequest=NativeUninstallRequest(packagePath:embedded,libraryPath:library,interactive:false)
assert(uninstallPackageIdentities(innerRequest,executingPath:browserHelper,owner:owner,inspect:{files[$0]})==nil)
let browserFiles=files
for missing in [browserRoot+"/Contents/MacOS/PhotoFindry",browserRoot+"/Contents/Resources/browser-host-manifest.json"] {
    files=browserFiles;files.removeValue(forKey:missing)
    assert(uninstallPackageIdentities(browserRequest,executingPath:browserHelper,owner:owner,inspect:{files[$0]})==nil)
}
files=browserFiles
files[browserRoot+"/Contents/Resources"]=NativeUninstallIdentity(device:1,inode:888,mode:UInt32(S_IFLNK)|0o777,owner:owner,size:20,modified:10)
assert(uninstallPackageIdentities(browserRequest,executingPath:browserHelper,owner:owner,inspect:{files[$0]})==nil)
print("Native uninstall checks passed; no prompt or filesystem mutation was invoked.")
`;
    const source = path.join(root, 'main.swift'), binary = path.join(root, 'uninstall');
    fs.writeFileSync(source, original.slice(0, original.indexOf(marker)) + driver);
    const compiled = spawnSync('/usr/bin/xcrun', ['swiftc', '-framework', 'AppKit', '-framework', 'CoreServices', source, '-o', binary], { encoding: 'utf8', timeout: 30000 });
    assert.equal(compiled.status, 0, compiled.stderr);
    const result = spawnSync(binary, [fs.realpathSync(root)], { encoding: 'utf8', timeout: 5000 });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.trim(), 'Native uninstall checks passed; no prompt or filesystem mutation was invoked.');
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

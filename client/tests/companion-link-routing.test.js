'use strict';
// Opt-in platform qualification. Only fresh UUID URL schemes and disposable
// bundles are registered. Never resets LaunchServices or touches product defaults.
const test=require('node:test'),assert=require('node:assert/strict');
const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),crypto=require('node:crypto');
const {spawnSync}=require('node:child_process');

test('website companion link survives deleted installer and an older app reclaiming legacy links',{
 skip:process.platform!=='darwin'||process.env.PHOTOFINDRY_QUALIFY_LAUNCH_SERVICES!=='1',timeout:120000
},()=>{
 const root=fs.realpathSync(fs.mkdtempSync(path.join(os.homedir(),'Library/Application Support/PhotoFindry Link Test-')));
 const id=crypto.randomUUID().toLowerCase(),currentScheme='pfcomp-'+id,legacyScheme='pflegacy-'+id,archiveScheme='pfarchive-'+id;
 const source=fs.readFileSync(path.join(__dirname,'../scripts/companion-launcher.swift'),'utf8');
 const helper=path.join(root,'installed/bin/CompanionLauncher.app'),installer=path.join(root,'PhotoFindry.app'),previous=path.join(root,'PhotoFindry-previous.app');
 const bundles=[helper,installer,previous],marker=path.join(root,'opened-command.txt');
 const run=(command,args,options={})=>{const native=command.startsWith(root)?['/bin/launchctl',['asuser',String(process.getuid()),command,...args]]:[command,args];const r=spawnSync(native[0],native[1],{encoding:'utf8',timeout:45000,...options});assert.equal(r.status,0,r.stderr||r.error?.message);return r.stdout.trim();};
 function bundle(where,bundleId,schemes,binary){
  fs.mkdirSync(path.join(where,'Contents/MacOS'),{recursive:true});fs.copyFileSync(binary,path.join(where,'Contents/MacOS/CompanionLauncher'));
  const info={CFBundleIdentifier:bundleId,CFBundleName:'PhotoFindry launch qualification',CFBundleExecutable:'CompanionLauncher',CFBundlePackageType:'APPL',CFBundleVersion:'1',CFBundleShortVersionString:'1.0',LSUIElement:true};
  if(schemes.length)info.CFBundleURLTypes=schemes.map(s=>({CFBundleURLSchemes:[s],CFBundleTypeRole:'Viewer'}));
  const json=path.join(where,'Contents/Info.json');fs.writeFileSync(json,JSON.stringify(info));run('/usr/bin/plutil',['-convert','xml1','-o',path.join(where,'Contents/Info.plist'),json]);fs.unlinkSync(json);
  run('/usr/bin/codesign',['--force','--sign','-',where]);
 }
 try{
  const helperSource=path.join(root,'helper.swift'),helperBinary=path.join(root,'helper');
  // Only isolate the fixed protocol literals. The native launch implementation
  // remains the production one; its sibling CLI is a recording test executable.
  const isolated=source.replaceAll('photofindry-companion',currentScheme).replaceAll('photofindry-local',legacyScheme).replaceAll('photofindry://',archiveScheme+'://').replaceAll('com.sentrybottale.photorecall.companion-launcher','com.photofindry.qualification.'+id+'.helper').replaceAll('("photofindry",','("'+archiveScheme+'",');
  const declarations=isolated.slice(isolated.indexOf('let companionLinks='),isolated.indexOf('func selectLocalLinkHandlers'));
  assert(!declarations.includes('\"photofindry'),'Test must never select production scheme defaults');
  fs.writeFileSync(helperSource,isolated);run('/usr/bin/xcrun',['swiftc','-framework','AppKit','-framework','CoreServices',helperSource,'-o',helperBinary]);
  bundle(helper,'com.photofindry.qualification.'+id+'.helper',[currentScheme,legacyScheme,archiveScheme],helperBinary);
  bundle(installer,'com.photofindry.qualification.'+id+'.installer',[legacyScheme],helperBinary);
  bundle(previous,'com.photofindry.qualification.'+id+'.previous',[legacyScheme],helperBinary);
  const bareHelper=path.join(root,'installed/bin/CompanionLauncher');fs.copyFileSync(helperBinary,bareHelper);
  fs.writeFileSync(path.join(root,'installed/bin/photofindry'),`#!/bin/sh\nprintf '%s\\n' "$@" > '${marker}'\n`,{mode:0o700});
  const driver=source.slice(0,source.indexOf('// Native launcher entry point.'))+String.raw`
let application=NSApplication.shared
application.setActivationPolicy(.accessory)
application.finishLaunching()
let args=CommandLine.arguments
let app=URL(fileURLWithPath:args[2])
if args[1]=="select" {
 guard LSRegisterURL(app as CFURL,true)==noErr else {exit(20)}
 let scheme=args[3]
 guard selectLocalLinkHandlers(app,links:[(scheme,scheme+"://open")],setHandler:{a,s,c in NSWorkspace.shared.setDefaultApplication(at:a,toOpenURLsWithScheme:s){error in if let error {FileHandle.standardError.write(Data(("SET: "+String(describing:error)+"\n").utf8))};c(error)}},resolve:{url in let value=NSWorkspace.shared.urlForApplication(toOpen:url);if let value {FileHandle.standardError.write(Data(("RESOLVE: "+value.absoluteString+" EXPECTED: "+app.absoluteString+"\n").utf8))};return value}) else {exit(21)}
} else if args[1]=="resolve" {
 guard let target=NSWorkspace.shared.urlForApplication(toOpen:URL(string:args[3]+"://open")!) else {exit(22)}
 print(target.resolvingSymlinksInPath().path)
} else if args[1]=="open" {
 guard NSWorkspace.shared.open(URL(string:args[3]+"://open")!) else {exit(23)}
} else {exit(24)}
`;
  const driverSource=path.join(root,'driver.swift'),rawDriver=path.join(root,'driver');fs.writeFileSync(driverSource,driver);run('/usr/bin/xcrun',['swiftc','-framework','AppKit','-framework','CoreServices',driverSource,'-o',rawDriver]);const driverBundle=path.join(root,'RegistrationDriver.app');bundle(driverBundle,'com.photofindry.qualification.'+id+'.driver',[],rawDriver);bundles.push(driverBundle);const driverBinary=path.join(driverBundle,'Contents/MacOS/CompanionLauncher');
  run(driverBinary,['select',previous,legacyScheme]);
  run(driverBinary,['select',installer,legacyScheme]);
  run(bareHelper,['--register-links']);
  assert.equal(run(driverBinary,['resolve',helper,currentScheme]),helper);
  // The migration application disappears, while a previous app remains and
  // actually reclaims the legacy default. The companion entry must be independent.
  fs.rmSync(installer,{recursive:true});run(driverBinary,['select',previous,legacyScheme]);
  assert.equal(run(driverBinary,['resolve',previous,legacyScheme]),previous);
  assert.equal(run(driverBinary,['resolve',helper,currentScheme]),helper);
  run(driverBinary,['open',helper,currentScheme]);
  const deadline=Date.now()+15000;while(!fs.existsSync(marker)&&Date.now()<deadline)Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0,100);
  assert(fs.existsSync(marker),'The actual OS URL dispatch did not reach the companion helper');
  assert.equal(fs.readFileSync(marker,'utf8'),'open-companion\n','Native entry must invoke only the companion open command');
 }finally{
  for(const app of bundles)if(fs.existsSync(app))spawnSync('/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister',['-u',app],{encoding:'utf8',timeout:10000});
  fs.rmSync(root,{recursive:true,force:true});
 }
});

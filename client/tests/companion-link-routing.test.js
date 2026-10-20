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
 const installation=path.join(root,'installed'),helper=path.join(installation,'entry/PhotoFindry.app'),packagedHelper=path.join(installation,'versions/120/package/bin/CompanionLauncher.app'),oldHelper=path.join(root,'OldCompanionLauncher.app'),duplicate=path.join(root,'OtherSameBuildLauncher.app'),installer=path.join(root,'PhotoFindry.app'),previous=path.join(root,'PhotoFindry-previous.app');
 const bundles=[helper,packagedHelper,oldHelper,duplicate,installer,previous],marker=path.join(root,'opened-command.txt'),executions=path.join(root,'opened-executable.txt');
 const signing=crypto.generateKeyPairSync('ed25519'),pin=signing.publicKey.export({format:'der',type:'spki'}).subarray(-32).toString('base64');
 const run=(command,args,options={})=>{const native=command.startsWith(root)?['/bin/launchctl',['asuser',String(process.getuid()),command,...args]]:[command,args];const r=spawnSync(native[0],native[1],{encoding:'utf8',timeout:45000,...options});assert.equal(r.status,0,r.stderr||r.error?.message);return r.stdout.trim();};
 function bundle(where,bundleId,schemes,binary,version='1'){
  fs.mkdirSync(path.join(where,'Contents/MacOS'),{recursive:true});fs.copyFileSync(binary,path.join(where,'Contents/MacOS/CompanionLauncher'));
  const info={CFBundleIdentifier:bundleId,CFBundleName:'PhotoFindry launch qualification',CFBundleExecutable:'CompanionLauncher',CFBundlePackageType:'APPL',CFBundleVersion:version,CFBundleShortVersionString:'1.0',LSUIElement:true};
  if(schemes.length)info.CFBundleURLTypes=schemes.map(s=>({CFBundleURLSchemes:[s],CFBundleTypeRole:'Viewer'}));
  const json=path.join(where,'Contents/Info.json');fs.writeFileSync(json,JSON.stringify(info));run('/usr/bin/plutil',['-convert','xml1','-o',path.join(where,'Contents/Info.plist'),json]);fs.unlinkSync(json);
  run('/usr/bin/codesign',['--force','--sign','-',where]);
 }
 try{
  const helperSource=path.join(root,'helper.swift'),helperBinary=path.join(root,'helper');
  // Only isolate the fixed protocol literals. The native launch implementation
  // remains the production one; its sibling CLI is a recording test executable.
  const recoveryHook=String.raw`
if Array(CommandLine.arguments.dropFirst()) == ["--test-recover"] {
 let app=NSApplication.shared,delegate=PhotoFindryLauncher()
 app.delegate=delegate;app.setActivationPolicy(.accessory);app.finishLaunching()
 delegate.openLibrary(.companion,recovering:true);app.run();exit(0)
}
`;
  const isolated=source.replace('G363x/YCT07qbJyyI5minQjp3v+6SEw9sB7dU3pL6cM=',pin).replace('private func openLibrary(', 'func openLibrary(').replace('// Native launcher entry point.',recoveryHook+'\n// Native launcher entry point.').replaceAll('photofindry-companion',currentScheme).replaceAll('photofindry-local',legacyScheme).replaceAll('photofindry://',archiveScheme+'://').replaceAll('com.sentrybottale.photorecall.companion-launcher','com.photofindry.qualification.'+id+'.helper').replaceAll('("photofindry",','("'+archiveScheme+'",');
  const declarations=isolated.slice(isolated.indexOf('let companionLinks='),isolated.indexOf('func selectLocalLinkHandlers'));
  assert(!declarations.includes('\"photofindry'),'Test must never select production scheme defaults');
  fs.writeFileSync(helperSource,isolated);run('/usr/bin/xcrun',['swiftc','-framework','AppKit','-framework','CoreServices',helperSource,'-o',helperBinary]);
  bundle(helper,'com.photofindry.companion.entry.'+id.replaceAll('-',''),[currentScheme,legacyScheme,archiveScheme],helperBinary,'120');
  bundle(packagedHelper,'com.photofindry.qualification.'+id+'.helper.b120',[],helperBinary,'120');
  bundle(oldHelper,'com.photofindry.qualification.'+id+'.helper',[currentScheme,legacyScheme,archiveScheme],helperBinary,'118');
  bundle(duplicate,'com.photofindry.qualification.'+id+'.helper',[currentScheme,legacyScheme,archiveScheme],helperBinary,'118');
  bundle(installer,'com.photofindry.qualification.'+id+'.installer',[legacyScheme],helperBinary);
  bundle(previous,'com.photofindry.qualification.'+id+'.previous',[legacyScheme],helperBinary);
  fs.chmodSync(installation,0o700);
  fs.mkdirSync(path.join(helper,'Contents/Resources'),{recursive:true});
  fs.writeFileSync(path.join(helper,'Contents/Resources/installed-entry.json'),JSON.stringify({schema:1,installRoot:installation}),{mode:0o600});
  run('/usr/bin/codesign',['--force','--sign','-',helper]);
  const stableHelper=path.join(helper,'Contents/MacOS/CompanionLauncher');
  function installBuild(build){
   const version=path.join(installation,'versions',String(build)),bin=path.join(version,'package/bin');fs.mkdirSync(bin,{recursive:true,mode:0o700});
   const bytes=Buffer.from(`#!/bin/sh\nprintf '%s\\n' "$@" >> '${marker}'\nprintf '%s\\n' "$0" >> '${executions}'\nif [ "$1" = lifecycle ] && [ "$2" = recover ]; then /bin/cp "$0" '${path.join(installation,'versions/121/package/bin/photofindry')}'; /bin/chmod 700 '${path.join(installation,'versions/121/package/bin/photofindry')}'; fi\n`);
   const hash=crypto.createHash('sha256').update(bytes).digest('hex'),rows=[];
   for(const name of ['photofindry','CompanionReleaseVerify','CompanionLauncher']){fs.writeFileSync(path.join(bin,name),bytes,{mode:0o700});rows.push(hash+'  bin/'+name);}
   const inventory=Buffer.from(rows.join('\n')+'\n');fs.writeFileSync(path.join(version,'package/SHA256SUMS.txt'),inventory,{mode:0o600});
   const payload=Buffer.from(JSON.stringify({schema:1,kind:'photofindry-cli-release',build,platform:'macos-arm64',dataContract:'photofindry-library-compatible-v1',inventorySHA256:crypto.createHash('sha256').update(inventory).digest('hex')}));
   fs.writeFileSync(path.join(version,'release.json'),JSON.stringify({payload:payload.toString('base64'),signature:crypto.sign(null,payload,signing.privateKey).toString('base64')}),{mode:0o600});
   fs.writeFileSync(path.join(installation,'installation.json'),JSON.stringify({schema:1,current:build,previous:build-1}),{mode:0o600});
  }
  installBuild(120);
  const resources=path.join(helper,'Contents/Resources');fs.mkdirSync(path.join(resources,'bootstrap'),{recursive:true});
  for(const name of ['photofindry','CompanionReleaseVerify','CompanionLauncher'])fs.copyFileSync(path.join(installation,'versions/120/package/bin',name),path.join(resources,'bootstrap',name));
  fs.copyFileSync(path.join(installation,'versions/120/release.json'),path.join(resources,'bootstrap-release.json'));
  fs.copyFileSync(path.join(installation,'versions/120/package/SHA256SUMS.txt'),path.join(resources,'bootstrap-SHA256SUMS.txt'));
  run('/usr/bin/codesign',['--force','--sign','-',helper]);
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
  run(driverBinary,['select',oldHelper,currentScheme]);
  run(path.join(packagedHelper,'Contents/MacOS/CompanionLauncher'),['--register-links']);
  assert.equal(fs.readFileSync(marker,'utf8'),`lifecycle\nrepair-entry\n--install-root\n${installation}\n`,'A legacy updater call delegates only to the authenticated current controller');
  assert.equal(run(driverBinary,['resolve',oldHelper,currentScheme]),oldHelper,'The generic packaged helper must never claim URL schemes');
  fs.unlinkSync(marker);fs.unlinkSync(executions);
  run(stableHelper,['--register-links']);
  assert.equal(run(driverBinary,['resolve',helper,currentScheme]),helper,'A stable per-install entry must win over competing same-build package copies');
  // The migration application disappears, while a previous app remains and
  // actually reclaims the legacy default. The companion entry must be independent.
  fs.rmSync(installer,{recursive:true});run(driverBinary,['select',previous,legacyScheme]);
  assert.equal(run(driverBinary,['resolve',previous,legacyScheme]),previous);
  assert.equal(run(driverBinary,['resolve',helper,currentScheme]),helper);
  run(driverBinary,['open',helper,currentScheme]);
  const deadline=Date.now()+15000;while(!fs.existsSync(marker)&&Date.now()<deadline)Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0,100);
  assert(fs.existsSync(marker),'The actual OS URL dispatch did not reach the companion helper');
  assert.equal(fs.readFileSync(marker,'utf8'),'open-companion\n','Native entry must invoke only the companion open command');
  // An update changes the authenticated current software, never the registered
  // entry path or per-install bundle identity. Reopening must follow that state.
  fs.unlinkSync(marker);installBuild(121);
  assert.equal(run(driverBinary,['resolve',helper,currentScheme]),helper);
  run(driverBinary,['open',helper,currentScheme]);
  const updatedDeadline=Date.now()+15000;while(!fs.existsSync(marker)&&Date.now()<updatedDeadline)Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0,100);
  assert(fs.existsSync(marker),'The stable entry did not open the newly authenticated current build');
  assert.equal(fs.readFileSync(marker,'utf8'),'open-companion\n');
  fs.unlinkSync(marker);fs.unlinkSync(executions);
  installBuild(119);fs.writeFileSync(path.join(installation,'installation.json'),JSON.stringify({schema:1,current:121,previous:119}),{mode:0o600});
  fs.unlinkSync(path.join(installation,'versions/121/package/bin/photofindry'));
  run(stableHelper,['--test-recover']);
  assert.equal(fs.readFileSync(marker,'utf8'),`lifecycle\nrecover\n--install-root\n${installation}\nopen-companion\n`);
  const invoked=fs.readFileSync(executions,'utf8').trim().split('\n');
  assert.equal(invoked[0],path.join(resources,'bootstrap/photofindry'),'Corrupt current with legacy previous119 must run the authenticated new recovery controller');
  assert.equal(invoked[1],path.join(installation,'versions/121/package/bin/photofindry'));
 }finally{
  for(const app of bundles)if(fs.existsSync(app))spawnSync('/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister',['-u',app],{encoding:'utf8',timeout:10000});
  fs.rmSync(root,{recursive:true,force:true});
 }
});

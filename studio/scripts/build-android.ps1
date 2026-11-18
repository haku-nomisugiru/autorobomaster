param(
  [string]$Jdk = $env:JAVA_HOME,
  [string]$Platform = "$env:ANDROID_HOME\platforms\android-35",
  [string]$BuildTools = "$env:ANDROID_HOME\build-tools\35.0.0",
  [string]$Zxing,
  [string]$Keystore,
  [string]$Alias = "studio"
)
$ErrorActionPreference='Stop'
$root = Split-Path -Parent $PSScriptRoot
$env:JAVA_HOME=$Jdk
$env:PATH=(Join-Path $Jdk 'bin')+';'+$env:PATH
if (!$Zxing) { $Zxing=Join-Path $root '.local\zxing-core.jar' }
if (!(Test-Path -LiteralPath $Zxing) -or (Get-FileHash -LiteralPath $Zxing -Algorithm SHA256).Hash -ne '8D8064C1636FDAEF7189DD9055C7D59950A8940A12F2293956446EC3C109FD82') {throw '需要 com.google.zxing:core:3.5.3 的原始 JAR'}
$androidJar = Join-Path $Platform 'android.jar'
$build=Join-Path $root ('.local\android-build-'+[guid]::NewGuid().ToString('N'))
$classes=Join-Path $build 'classes'
$dex=Join-Path $build 'dex'
New-Item -ItemType Directory -Path $classes,$dex | Out-Null
function Run([string]$Program,[string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE) {throw "$Program exited with $LASTEXITCODE"}
}
$source=Join-Path $root 'android'
Run "$BuildTools\aapt2.exe" @('compile','--dir',"$source\res",'-o',"$build\res.zip")
Run "$BuildTools\aapt2.exe" @('link','-I',$androidJar,'--manifest',"$source\AndroidManifest.xml",'-A',"$source\assets",'-o',"$build\base.apk","$build\res.zip")
$javaFiles=@(Get-ChildItem -Path "$source\src" -Recurse -Filter '*.java' | ForEach-Object FullName)
Run "$Jdk\bin\javac.exe" (@('-encoding','UTF-8','--release','8','-classpath',"$androidJar;$Zxing",'-d',$classes)+$javaFiles)
Run "$Jdk\bin\jar.exe" @('cf',"$build\console.jar",'-C',$classes,'.')
Run "$BuildTools\d8.bat" @('--min-api','26','--lib',$androidJar,'--output',$dex,"$build\console.jar",$Zxing)
Run "$Jdk\bin\jar.exe" @('uf',"$build\base.apk",'-C',$dex,'classes.dex')
Run "$BuildTools\zipalign.exe" @('-p','-f','4',"$build\base.apk","$build\aligned.apk")
if (!$Keystore) {
    $keyDir=Join-Path $root '.local\android-signing'
    New-Item -ItemType Directory -Force $keyDir | Out-Null
    $Keystore=Join-Path $keyDir 'studio.p12'
    $pwFile=Join-Path $keyDir 'local-password.txt'
    if (!(Test-Path -LiteralPath $Keystore)) {
        $password=[Guid]::NewGuid().ToString('N')+[Guid]::NewGuid().ToString('N')
        [IO.File]::WriteAllText($pwFile,$password)
        $env:STUDIO_APK_PASS=$password
        Run "$Jdk\bin\keytool.exe" @('-genkeypair','-noprompt','-keystore',$Keystore,'-storetype','PKCS12','-storepass:env','STUDIO_APK_PASS','-keypass:env','STUDIO_APK_PASS','-alias',$Alias,'-keyalg','RSA','-keysize','3072','-validity','8000','-dname','CN=RoboMaster Studio Local Build')
    } else {$env:STUDIO_APK_PASS=[IO.File]::ReadAllText($pwFile)}
}
try {
    $releaseDir = New-Item -ItemType Directory -Force (Join-Path $root 'release')
    $out=Join-Path $releaseDir.FullName 'RoboMaster-Console-v0.3.0.apk'
    Run "$BuildTools\apksigner.bat" @('sign','--ks',$Keystore,'--ks-key-alias',$Alias,'--ks-pass','env:STUDIO_APK_PASS','--out',$out,"$build\aligned.apk")
    Run "$BuildTools\apksigner.bat" @('verify','--verbose',$out)
    Get-FileHash -LiteralPath $out -Algorithm SHA256
} finally {Remove-Item Env:STUDIO_APK_PASS -ErrorAction SilentlyContinue}


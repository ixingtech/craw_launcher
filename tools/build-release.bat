@echo off
setlocal

set "ROOT=%~dp0.."
pushd "%ROOT%" >nul

set "TARGET=%~1"
set "HANDLED="
if "%TARGET%"=="" goto :usage

if /I "%TARGET%"=="windows-release-zh" set "HANDLED=1" & call :run pnpm build:release:zh-CN
if /I "%TARGET%"=="windows-release-en" set "HANDLED=1" & call :run pnpm build:release:en-US
if /I "%TARGET%"=="windows-release-all" set "HANDLED=1" & call :run pnpm build:release:zh-CN && call :run pnpm build:release:en-US
if /I "%TARGET%"=="windows-installer-zh" set "HANDLED=1" & call :run pnpm build:nsis:zh-CN
if /I "%TARGET%"=="windows-installer-en" set "HANDLED=1" & call :run pnpm build:nsis:en-US
if /I "%TARGET%"=="windows-installer-all" set "HANDLED=1" & call :run pnpm build:nsis:zh-CN && call :run pnpm build:nsis:en-US
if /I "%TARGET%"=="mac-zh" set "HANDLED=1" & call :run pnpm build:mac:zh-CN
if /I "%TARGET%"=="mac-en" set "HANDLED=1" & call :run pnpm build:mac:en-US
if /I "%TARGET%"=="mac-all" set "HANDLED=1" & call :run pnpm build:mac:zh-CN && call :run pnpm build:mac:en-US
if /I "%TARGET%"=="cli-zh" set "HANDLED=1" & call :run pnpm build:cli:archive:zh-CN
if /I "%TARGET%"=="cli-en" set "HANDLED=1" & call :run pnpm build:cli:archive:en-US
if /I "%TARGET%"=="cli-all" set "HANDLED=1" & call :run pnpm build:cli:archive:zh-CN && call :run pnpm build:cli:archive:en-US
if /I "%TARGET%"=="all" set "HANDLED=1" & call :run pnpm build:release:zh-CN && call :run pnpm build:release:en-US && call :run pnpm build:nsis:zh-CN && call :run pnpm build:nsis:en-US && call :run pnpm build:mac:zh-CN && call :run pnpm build:mac:en-US && call :run pnpm build:cli:archive:zh-CN && call :run pnpm build:cli:archive:en-US

if not defined HANDLED goto :usage

if not errorlevel 1 goto :done
goto :fail

:run
echo.
echo ===^> %*
call %*
exit /b %errorlevel%

:usage
echo Usage: tools\build-release.bat ^<target^>
echo.
echo Targets:
echo   windows-release-zh
echo   windows-release-en
echo   windows-release-all
echo   windows-installer-zh
echo   windows-installer-en
echo   windows-installer-all
echo   mac-zh
echo   mac-en
echo   mac-all
echo   cli-zh
echo   cli-en
echo   cli-all
echo   all
exit /b 1

:fail
set "CODE=%errorlevel%"
popd >nul
exit /b %CODE%

:done
popd >nul
exit /b 0

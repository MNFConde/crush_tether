@echo off
rem crush-tether loader-guard wrapper (design.md "loader guard"): dumb by
rem design -- forward args/stdio/exit code when the binary is found on PATH;
rem exit 2 with install guidance when missing (failure mode #2: turn silent
rem fail-open into a loud failure). Never parses hook envelopes.
rem NOTE: keep this file pure ASCII -- cmd.exe parses batch files in the OEM
rem codepage, non-ASCII comments get mangled into bogus commands.
rem zcode plugin track launches this file via the ${ZCODE_PLUGIN_ROOT} absolute
rem path; arg vector is fixed (hook --agent zcode).
where crush-tether.exe >NUL 2>&1
if %ERRORLEVEL% NEQ 0 (
  echo crush-tether: binary not found on PATH; install with: cargo install --path . 1>&2
  exit /b 2
)
crush-tether %*
exit /b %ERRORLEVEL%

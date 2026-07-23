# Place the official Prolific USB-to-Serial driver installer here.
#
# Download (Windows 7/10/11 x64 WHQL):
#   https://www.prolific.com.tw/US/ShowProduct.aspx?p_id=225&pcid=41
#   Product line: PL2303 / PL23XX USB-to-Serial
#
# Save / rename the downloaded EXE to exactly:
#   PL23XX_Prolific_DriverInstaller.exe
#
# Or run:  .\installer\build-installer.ps1
# which will pick up any PL23XX*.exe / *Prolific*DriverInstaller*.exe
# in this folder and rename it to the name above.
#
# Silent install (used by our setup):  PL23XX_Prolific_DriverInstaller.exe /s
#
# Do NOT commit the .exe to git (large, proprietary). See .gitignore.

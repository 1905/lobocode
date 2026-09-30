# Check Mac memory before local Start

**Date:** 2026-09-30
**Status:** done

The user wants the app to check memory on the Mac before local startup.
If the selected model cannot fit, show an error and do not start it.
Use current memory availability, not only installed RAM or model file size.
Keep the check independent of model loading and downloads.
Keep the native view compact, with no scrolling.
Run backend tests on Dell. Keep inference off this Mac and preserve the release hold.

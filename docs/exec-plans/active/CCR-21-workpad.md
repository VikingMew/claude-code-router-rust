# CCR-21 Workpad

**Status:** implementation workpad
**Last updated:** 2026-05-21

## Interpretation

The newest human Linear comment says to decide whether legacy scenario routing is compatibility to remove/document, or to reintroduce it only as Route Pool policy/rule. This implementation treats `background`, `think`, `webSearch`, `longContext`, `image`, project router override and custom router override as not-current runtime goals. The code path remains Route Pool-only, so unused `ccr-router` primary/scenario routing APIs are removed and surviving config fields are documented as legacy deserialization compatibility.

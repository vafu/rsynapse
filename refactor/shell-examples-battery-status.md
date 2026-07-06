# shell-examples-battery-status Review

Status: reviewed

## Scope

Example shell widget/application for battery status.

## Findings

- No correctness findings in this pass. The example uses the shared D-Bus
  property source helper at `shell/examples/battery-status/src/main.rs:238`,
  which makes it a reasonable template for simple property-composed widgets.

## Refactor Ideas

- Add a small unit test for `BatteryState::from` and `BatteryStatus::fraction`.
  The package currently compiles under test but has no behavioral tests.
- If this graduates beyond an example, consider keeping the generated UPower
  proxy and display-device model in a tiny reusable module so other shell
  surfaces do not duplicate the same property constants.

## Open Questions

- Should examples intentionally keep raw `zbus::proxy` declarations inline for
  readability, or should they demonstrate the same shared-client shape expected
  in app code?

## Verification

- `cargo test -p rsynapse-battery-status-example --manifest-path shell/Cargo.toml`
  passed; 0 tests.

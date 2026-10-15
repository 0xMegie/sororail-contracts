# Vesting Arithmetic Safe Range

`Grant::vested_amount` uses checked i128 arithmetic for the linear vesting step:

```text
total * elapsed / duration
```

The largest safe `total` before the multiplication overflows is:

```text
i128::MAX / elapsed
```

For a ten-year grant, the largest elapsed value before the end shortcut is roughly 315,359,999 seconds. That leaves a safe total of about `5.39e29` base units. A common launch-sized grant such as one billion 18-decimal tokens (`1e27` base units) is comfortably inside that range, and the vesting test suite now covers that scenario directly.

At or after the grant end timestamp, vesting returns the full grant amount before performing the multiplication, so the final timestamp does not create an additional overflow path.
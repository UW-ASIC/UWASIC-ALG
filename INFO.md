# NgSpice Test Writing Guide

## Overview

Tests run in **interactive mode** - each test executes its own analysis separately. Write simple, direct measurement commands.

## Test Structure

```python
Test(
    name="test_name",
    environment=[],  # Optional: temperature, supply voltage, etc.
    spice_code="""
.ac dec 100 1 1G
meas ac my_metric_val FIND vdb(vout) AT=10
print my_metric_val
""",
    description="What this test measures"
)
```

## Key Rules

### 1. Metric Naming Convention
Metrics **MUST** be named `{metric.lower()}_val`:
- Target: `metric="DC_GAIN"` → Variable: `dc_gain_val`
- Target: `metric="POWER"` → Variable: `power_val`

### 2. Analysis Directives
Use **netlist format** (with dot prefix):
- `.ac dec 100 1 1G` - AC analysis
- `.dc Vin 0 1.8 0.01` - DC sweep
- `.tran 1n 100n` - Transient
- `.op` - Operating point

These are automatically converted to interactive commands (`ac`, `dc`, `tran`, `op`).

### 3. No Control Flow
❌ **DON'T use** `if`/`else`/`end` - they don't work in interactive mode.

✅ **DO use** simple measurements and calculations:
```spice
meas ac dc_gain_val FIND vdb(vout) AT=10
let power_val = v(vdd) * (-i(V2))
```

### 4. Skip Control Blocks
Don't include `.control`/`.endc` or `run` - they're handled automatically.

## Working Examples

### Example 1: DC Gain
```python
Test(
    name="dc_gain_measurement",
    environment=[],
    spice_code="""
.ac dec 100 1 1G
meas ac dc_gain_val FIND vdb(vout) AT=10
print dc_gain_val
""",
    description="Measure DC gain at 10 Hz"
)
```

### Example 2: Power Consumption
```python
Test(
    name="power_measurement",
    environment=[],
    spice_code="""
.op
let power_val = v(vdd) * (-i(V2))
print power_val
""",
    description="Measure DC power"
)
```

### Example 3: Bandwidth (with calculation)
```python
Test(
    name="gbw_measurement",
    environment=[],
    spice_code="""
.ac dec 100 1 1G
meas ac dc_gain_for_gbw FIND vdb(vout) AT=10
let gbw_val = 10 * 10^((dc_gain_for_gbw - 0) / 20)
print gbw_val
""",
    description="Estimate GBW from DC gain"
)
```

### Example 4: Phase Margin
```python
Test(
    name="phase_margin_measurement",
    environment=[],
    spice_code="""
.ac dec 100 1 1G
meas ac phase_rad FIND vp(vout) AT=1e6
let phase_margin_val = (180 + phase_rad * 180 / pi)
print phase_margin_val
""",
    description="Phase at 1MHz converted to degrees"
)
```

## Common Measurement Commands

### Find value at specific point
```spice
meas ac gain FIND vdb(vout) AT=1e6
meas dc current FIND i(R1) AT=0.9
meas tran max_v FIND v(out) AT=50n
```

### Find when expression crosses threshold
```spice
meas ac ugf WHEN vdb(vout)=0 CROSS=1
meas dc threshold WHEN v(out)=0.9 CROSS=1
```

### Math operations
```spice
let power_val = v(vdd) * abs(i(V2))
let phase_deg = vp(vout) * 180 / pi
let gain_linear = 10^(vdb(vout) / 20)
```

## Multiple Tests

Each test runs independently with its own analysis:

```python
tests = [
    Test(name="gain", spice_code=".ac dec 100 1 1G\nmeas ac dc_gain_val ..."),
    Test(name="phase", spice_code=".ac dec 100 1 1G\nmeas ac phase_val ..."),
    Test(name="power", spice_code=".op\nlet power_val ..."),
]
```

All tests execute in sequence for each optimization iteration.

## Common Issues

### Issue: Measurement returns penalty value
**Cause:** Measurement failed (threshold not crossed, frequency out of range)

**Solution:** Use simpler measurements or adjust frequency/threshold:
```spice
# Instead of:
meas ac ugf WHEN vdb(vout)=0 CROSS=1  # Might fail

# Try:
meas ac phase_val FIND vp(vout) AT=1e6  # Always succeeds if simulation runs
```

### Issue: Wrong metric values
**Cause:** Variable name doesn't match `{metric}_val` pattern

**Solution:** Check naming:
```python
Target(metric="DC_GAIN", ...)  # Target name

# Variable must be:
meas ac dc_gain_val ...  # Lowercase + _val
```

## Summary

✅ **DO:**
- Use `.ac`/`.dc`/`.tran`/`.op` analysis directives
- Name variables `{metric.lower()}_val`
- Use simple `meas` and `let` statements
- Include `print` statements for debugging

❌ **DON'T:**
- Use `if`/`else`/`end` control flow
- Include `.control`/`.endc` blocks
- Use `run` command
- Forget the `_val` suffix

## Minimal Working Template

```python
Test(
    name="my_test",
    environment=[],
    spice_code="""
.ac dec 100 1 1G
meas ac my_metric_val FIND vdb(vout) AT=10
print my_metric_val
""",
    description="Brief description"
)
```

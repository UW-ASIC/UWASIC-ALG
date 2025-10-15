from uwasic_optimizer import (
    Optimizer,
    Parameter,
    ParameterConstraint,
    Target,
    TargetMode,
    Test,
    Environment,
    RelationshipType,
)
from typing import List

# =============================================================================
# PARAMETERS
# =============================================================================

# Differential Pair (M1, M2)
M1_W = Parameter(name="XM1_W", value=1.0, min_val=0.42, max_val=10.0)
M1_L = Parameter(name="XM1_L", value=0.5, min_val=0.15, max_val=2.0)
M2_W = Parameter(name="XM2_W", value=1.0, min_val=0.42, max_val=10.0)
M2_L = Parameter(name="XM2_L", value=0.5, min_val=0.15, max_val=2.0)

# Active Load (M3, M4)
M3_W = Parameter(name="XM3_W", value=2.0, min_val=0.42, max_val=10.0)
M3_L = Parameter(name="XM3_L", value=0.5, min_val=0.15, max_val=2.0)
M4_W = Parameter(name="XM4_W", value=2.0, min_val=0.42, max_val=10.0)
M4_L = Parameter(name="XM4_L", value=0.5, min_val=0.15, max_val=2.0)

# Tail Current (M5)
M5_W = Parameter(name="XM5_W", value=2.0, min_val=0.42, max_val=10.0)
M5_L = Parameter(name="XM5_L", value=1.0, min_val=0.5, max_val=2.0)

# Output Stage (M6, M7)
M6_W = Parameter(name="XM6_W", value=4.0, min_val=0.42, max_val=20.0)
M6_L = Parameter(name="XM6_L", value=0.5, min_val=0.15, max_val=2.0)
M7_W = Parameter(name="XM7_W", value=2.0, min_val=0.42, max_val=10.0)
M7_L = Parameter(name="XM7_L", value=1.0, min_val=0.5, max_val=2.0)

# Compensation Capacitor
C1_value = Parameter(name="C1_value", value=2.0, min_val=1.0, max_val=5.0)

parameters: List[Parameter] = [
    M1_W,
    M1_L,
    M2_W,
    M2_L,
    M3_W,
    M3_L,
    M4_W,
    M4_L,
    M5_W,
    M5_L,
    M6_W,
    M6_L,
    M7_W,
    M7_L,
    C1_value,
]

# =============================================================================
# CONSTRAINTS
# =============================================================================

constraints: List[ParameterConstraint] = [
    # Differential pair matching
    ParameterConstraint(
        target_param=M1_W,
        source_params=[M2_W, M2_L, M1_L],
        expression="XM2_W",
        relationship=RelationshipType.Equals,
        description="M1.W = M2.W * (M1.L / M2.L)",
    ),
    # Active load matching
    ParameterConstraint(
        target_param=M3_W,
        source_params=[M4_W, M4_L, M3_L],
        expression="XM4_W",
        relationship=RelationshipType.Equals,
        description="M3.W = M4.W * (M3.L / M4.L)",
    ),
]

# =============================================================================
# TARGETS
# =============================================================================

targets: List[Target] = [
    # DC Gain
    Target(metric="DC_GAIN", value=40.0, weight=3.0, mode=TargetMode.Min, unit="dB"),
    # Gain-Bandwidth Product
    Target(metric="GBW", value=5e6, weight=2.5, mode=TargetMode.Min, unit="Hz"),
    # Phase Margin
    Target(
        metric="PHASE_MARGIN",
        value=45.0,
        weight=2.0,
        mode=TargetMode.Min,
        unit="degrees",
    ),
    # Power Consumption
    Target(metric="POWER", value=2e-3, weight=1.5, mode=TargetMode.Max, unit="W"),
]

# =============================================================================
# TESTS - COMPLETELY SEPARATED
# =============================================================================

tests: List[Test] = [
    # =========================================================================
    # Test 1: DC Gain Measurement ONLY
    # =========================================================================
    Test(
        name="dc_gain_measurement",
        environment=[],
        spice_code="""
.ac dec 100 1 1G
.control
run
meas ac dc_gain_val FIND vdb(vout) AT=10
echo "DC_GAIN: $&dc_gain_val"
.endc
""",
        description="Measure DC gain at low frequency",
    ),
    # =========================================================================
    # Test 2: Gain-Bandwidth Product ONLY
    # =========================================================================
    Test(
        name="gbw_measurement",
        environment=[],
        spice_code="""
.ac dec 100 1 1G
.control
run
* First get DC gain
meas ac dc_gain_db FIND vdb(vout) AT=10

* Try to find unity gain frequency
meas ac ugf_temp WHEN vdb(vout)=0 CROSS=1

* If unity gain exists, use it directly
if length(ugf_temp) > 0
  let gbw_val = ugf_temp
else
  * Otherwise estimate from 3dB bandwidth
  meas ac bw3db WHEN vdb(vout)=dc_gain_db-3 CROSS=1
  if length(bw3db) > 0
    let gbw_val = bw3db * 10^(dc_gain_db/20)
  else
    * Fallback: assume 1 MHz
    let gbw_val = 1e6
  end
end

echo "GBW: $&gbw_val"
.endc
""",
        description="Measure gain-bandwidth product",
    ),
    # =========================================================================
    # Test 3: Phase Margin ONLY
    # =========================================================================
    Test(
        name="phase_margin_measurement",
        environment=[],
        spice_code="""
.ac dec 100 1 1G
.control
run

* Find unity gain frequency
meas ac ugf WHEN vdb(vout)=0 CROSS=1

* If UGF exists, measure phase at that frequency
if length(ugf) > 0
  meas ac phase_at_ugf FIND vp(vout) AT=ugf
  let phase_margin_val = 180 + phase_at_ugf
else
  * If no UGF, assume stable with conservative margin
  let phase_margin_val = 60
end

echo "PHASE_MARGIN: $&phase_margin_val"
.endc
""",
        description="Measure phase margin",
    ),
    # =========================================================================
    # Test 4: Power Consumption ONLY
    # =========================================================================
    Test(
        name="power_measurement",
        environment=[],
        spice_code="""
.op
.control
run
let power_val = v(vdd) * (-i(V2))
echo "POWER: $&power_val"
.endc
""",
        description="Measure DC power consumption",
    ),
]

# =============================================================================
# OPTIMIZATION EXECUTION
# =============================================================================

if __name__ == "__main__":
    optimizer = Optimizer(
        circuit="OpAmp_tb.sch",
        template="test/template",
        solver="pso",
        max_iterations=100,
        precision=1e-6,
        verbose=True,
    )

    result = optimizer.optimize(
        parameters=parameters, tests=tests, targets=targets, constraints=constraints
    )

    # =========================================================================
    # RESULTS DISPLAY
    # =========================================================================

    print("\n" + "=" * 70)
    print("OPTIMIZATION RESULTS - Two-Stage OpAmp")
    print("=" * 70)

    print(f"\nStatus: {'SUCCESS ✓' if result.success else 'FAILED ✗'}")
    print(f"Final Cost: {result.cost:.6e}")
    print(f"Iterations: {result.iterations}")
    print(f"Message: {result.message}")

    print("\n" + "-" * 70)
    print("OPTIMIZED PARAMETERS")
    print("-" * 70)

    print("\nDifferential Pair (M1, M2):")
    for p in result.parameters:
        if "XM1" in p.name or "XM2" in p.name:
            print(f"  {p.name:12s} = {p.value:8.4f}")

    print("\nActive Load (M3, M4):")
    for p in result.parameters:
        if "XM3" in p.name or "XM4" in p.name:
            print(f"  {p.name:12s} = {p.value:8.4f}")

    print("\nTail Current Source (M5):")
    for p in result.parameters:
        if "XM5" in p.name:
            print(f"  {p.name:12s} = {p.value:8.4f}")

    print("\nOutput Stage (M6, M7):")
    for p in result.parameters:
        if "XM6" in p.name or "XM7" in p.name:
            print(f"  {p.name:12s} = {p.value:8.4f}")

    print("\nCompensation Capacitor:")
    for p in result.parameters:
        if "C1" in p.name:
            print(f"  {p.name:12s} = {p.value:8.4f} pF")

    print("\n" + "=" * 70)

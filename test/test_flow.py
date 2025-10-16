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
# PARAMETERS - Two-Stage OpAmp Sizing
# =============================================================================

parameters: List[Parameter] = [
    # Differential Pair (M1, M2) - NFET
    # Moderate W for good gm, longer L for matching
    Parameter(name="XM1_W", value=5.0, min_val=0.42, max_val=50.0),  # um
    Parameter(name="XM1_L", value=1.0, min_val=0.15, max_val=10.0),  # um
    Parameter(name="XM2_W", value=5.0, min_val=0.42, max_val=50.0),  # um
    Parameter(name="XM2_L", value=1.0, min_val=0.15, max_val=10.0),  # um
    # Active Load (M3, M4) - PFET
    # Wider for current mirror, longer L for high output impedance
    Parameter(name="XM3_W", value=8.0, min_val=0.42, max_val=50.0),  # um
    Parameter(name="XM3_L", value=1.5, min_val=0.15, max_val=10.0),  # um
    Parameter(name="XM4_W", value=8.0, min_val=0.42, max_val=50.0),  # um
    Parameter(name="XM4_L", value=1.5, min_val=0.15, max_val=10.0),  # um
    # Tail Current Source (M5) - NFET
    # Moderate W, longer L for stable current source
    Parameter(name="XM5_W", value=4.0, min_val=0.42, max_val=50.0),  # um
    Parameter(name="XM5_L", value=2.0, min_val=0.15, max_val=10.0),  # um
    # Output Stage (M6, M7) - PFET and NFET
    # M6 (PFET): Large for high drive, moderate L
    Parameter(name="XM6_W", value=20.0, min_val=0.42, max_val=100.0),  # um
    Parameter(name="XM6_L", value=1.0, min_val=0.15, max_val=10.0),  # um
    # M7 (NFET): Half of M6 for proper biasing, longer L
    Parameter(name="XM7_W", value=10.0, min_val=0.42, max_val=50.0),  # um
    Parameter(name="XM7_L", value=2.0, min_val=0.15, max_val=10.0),  # um
    # Compensation Capacitor - typical miller cap
    Parameter(name="C1_value", value=3.0, min_val=0.5, max_val=20.0),  # pF
]

# =============================================================================
# CONSTRAINTS - Matching Requirements
# =============================================================================

constraints: List[ParameterConstraint] = [
    # Differential pair matching
    ParameterConstraint(
        target_param=parameters[0],  # XM1_W
        source_params=[parameters[2], parameters[1], parameters[3]],  # XM2_W
        expression="XM2_W * (XM1_L / XM2_L)",  # XM1_W = XM2_W * (XM1_L / XM2_L)
        relationship=RelationshipType.Equals,
        description="M1.W = M2.W (differential pair matching)",
    ),
]

# =============================================================================
# TARGETS - Performance Specifications
# =============================================================================

targets: List[Target] = [
    Target(
        metric="DC_GAIN",
        value=40.0,  # 40 dB minimum
        weight=3.0,
        mode=TargetMode.Min,
        unit="dB",
    ),
    Target(
        metric="GBW",
        value=5e6,  # 5 MHz minimum
        weight=2.0,
        mode=TargetMode.Min,
        unit="Hz",
    ),
    Target(
        metric="PHASE_MARGIN",
        value=45.0,  # 45 degrees minimum
        weight=2.0,
        mode=TargetMode.Min,
        unit="degrees",
    ),
    Target(
        metric="POWER",
        value=2e-3,  # 2 mW maximum
        weight=1.0,
        mode=TargetMode.Max,
        unit="W",
    ),
    Target(
        metric="AREA",
        value=50e-12,  # 50 um^2 maximum
        weight=1.5,
        mode=TargetMode.Max,
        unit="m^2",
    ),
]

# =============================================================================
# TESTS - Multi-Test with Environments
# =============================================================================

tests: List[Test] = [
    # =========================================================================
    # Test 1: DC Gain Measurement (Typical Corner, 27C, Nominal VDD)
    # =========================================================================
    Test(
        name="dc_gain_typical",
        environment=[
            Environment(name="temp", value="27"),
            Environment(name="V1", value="DC 0.9V"),
            Environment(name="V2", value="DC 1.8V"),
            Environment(name="V3", value="DC 0V AC 1mV"),
            Environment(name="V4", value="DC 0.7V"),
            Environment(name="V5", value="DC 0V AC 1mV"),
        ],
        spice_code="""
.ac dec 100 1 1G
meas ac dc_gain_val FIND vdb(vout) AT=10
print dc_gain_val
""",
        description="Measure DC gain at typical corner (27C, VDD=1.8V)",
    ),
    # =========================================================================
    # Test 2: GBW Measurement (Typical Corner, 27C, Nominal VDD)
    # =========================================================================
    Test(
        name="gbw_typical",
        environment=[
            Environment(name="temp", value="27"),
            Environment(name="V1", value="DC 0.9V"),
            Environment(name="V2", value="DC 1.8V"),
            Environment(name="V3", value="DC 0V AC 1mV"),
            Environment(name="V4", value="DC 0.7V"),
            Environment(name="V5", value="DC 0V AC 1mV"),
        ],
        spice_code="""
.ac dec 100 1 1G
meas ac dc_gain_for_gbw FIND vdb(vout) AT=10
let gbw_val = 10 * 10^((dc_gain_for_gbw - 0) / 20)
print gbw_val
""",
        description="Estimate GBW from DC gain (27C, VDD=1.8V)",
    ),
    # =========================================================================
    # Test 3: Phase Margin (Typical Corner, 27C, Nominal VDD)
    # =========================================================================
    Test(
        name="phase_margin_typical",
        environment=[
            Environment(name="temp", value="27"),
            Environment(name="V1", value="DC 0.9V"),
            Environment(name="V2", value="DC 1.8V"),
            Environment(name="V3", value="DC 0V AC 1mV"),
            Environment(name="V4", value="DC 0.7V"),
            Environment(name="V5", value="DC 0V AC 1mV"),
        ],
        spice_code="""
.ac dec 100 1 1G
meas ac phase_rad FIND vp(vout) AT=1e6
let phase_margin_val = (180 + phase_rad * 180 / pi)
print phase_margin_val
""",
        description="Measure phase at 1MHz (27C, VDD=1.8V)",
    ),
    # =========================================================================
    # Test 4: Power Consumption (Typical Corner, 27C, Nominal VDD)
    # =========================================================================
    Test(
        name="power_typical",
        environment=[
            Environment(name="temp", value="27"),
            Environment(name="V1", value="DC 0.9V"),
            Environment(name="V2", value="DC 1.8V"),
            Environment(name="V3", value="DC 0V AC 1mV"),
            Environment(name="V4", value="DC 0.7V"),
            Environment(name="V5", value="DC 0V AC 1mV"),
        ],
        spice_code="""
.op
let power_val = v(vdd) * (-i(V2))
print power_val
""",
        description="Measure DC power (27C, VDD=1.8V)",
    ),
    # =========================================================================
    # Test 5: Area Calculation
    # =========================================================================
    Test(
        name="area_calculation",
        environment=[
            Environment(name="temp", value="27"),
            Environment(name="V1", value="DC 0.9V"),
            Environment(name="V2", value="DC 1.8V"),
            Environment(name="V3", value="DC 0V AC 1mV"),
            Environment(name="V4", value="DC 0.7V"),
            Environment(name="V5", value="DC 0V AC 1mV"),
        ],
        spice_code="""
.op
* Calculate total transistor area (W*L for all transistors)
let area_m1 = @m.x1.xm1.msky130_fd_pr__nfet_01v8[w] * @m.x1.xm1.msky130_fd_pr__nfet_01v8[l]
let area_m2 = @m.x1.xm2.msky130_fd_pr__nfet_01v8[w] * @m.x1.xm2.msky130_fd_pr__nfet_01v8[l]
let area_m3 = @m.x1.xm3.msky130_fd_pr__pfet_01v8[w] * @m.x1.xm3.msky130_fd_pr__pfet_01v8[l]
let area_m4 = @m.x1.xm4.msky130_fd_pr__pfet_01v8[w] * @m.x1.xm4.msky130_fd_pr__pfet_01v8[l]
let area_m5 = @m.x1.xm5.msky130_fd_pr__nfet_01v8[w] * @m.x1.xm5.msky130_fd_pr__nfet_01v8[l]
let area_m6 = @m.x1.xm6.msky130_fd_pr__pfet_01v8[w] * @m.x1.xm6.msky130_fd_pr__pfet_01v8[l]
let area_m7 = @m.x1.xm7.msky130_fd_pr__nfet_01v8[w] * @m.x1.xm7.msky130_fd_pr__nfet_01v8[l]
let area_val = area_m1 + area_m2 + area_m3 + area_m4 + area_m5 + area_m6 + area_m7
print area_val
""",
        description="Calculate total transistor area (sum of W*L)",
    ),
]

# =============================================================================
# OPTIMIZATION EXECUTION
# =============================================================================

if __name__ == "__main__":
    print("=" * 80)
    print("UWASIC OPTIMIZER - Two-Stage OpAmp Design")
    print("=" * 80)
    print("\n📊 Configuration:")
    print(f"  Parameters: {len(parameters)}")
    print(f"  Constraints: {len(constraints)}")
    print(f"  Tests: {len(tests)}")
    print(f"  Targets: {len(targets)}")
    print()

    optimizer = Optimizer(
        circuit="OpAmp_tb.sch",
        template="test/template",
        solver="pso",
        max_iterations=3,
        precision=1e-6,
        verbose=True,
    )

    result = optimizer.optimize(
        parameters=parameters,
        tests=tests,
        targets=targets,
        constraints=constraints,
    )

    # =========================================================================
    # RESULTS DISPLAY
    # =========================================================================

    print("\n" + "=" * 80)
    print("OPTIMIZATION RESULTS - Two-Stage OpAmp")
    print("=" * 80)

    print(f"\nStatus: {'✓ SUCCESS' if result.success else '✗ FAILED'}")
    print(f"Final Cost: {result.cost:.6e}")
    print(f"Iterations: {result.iterations}")
    print(f"Message: {result.message}")

    print("\n" + "-" * 80)
    print("OPTIMIZED PARAMETERS")
    print("-" * 80)

    print("\nDifferential Pair (M1, M2):")
    for p in result.parameters[:4]:
        print(f"  {p.name:12s} = {p.value:8.4f}")

    print("\nActive Load (M3, M4):")
    for p in result.parameters[4:8]:
        print(f"  {p.name:12s} = {p.value:8.4f}")

    print("\nTail Current Source (M5):")
    for p in result.parameters[8:10]:
        print(f"  {p.name:12s} = {p.value:8.4f}")

    print("\nOutput Stage (M6, M7):")
    for p in result.parameters[10:14]:
        print(f"  {p.name:12s} = {p.value:8.4f}")

    print("\nCompensation Capacitor:")
    print(f"  {result.parameters[14].name:12s} = {result.parameters[14].value:8.4f} pF")

    print("\n" + "=" * 80)

import pytest
from uwasic_optimizer import (
    Optimizer,
    Parameter,
    Target,
    TargetMode,
    Test,
    Environment,
    ParameterConstraint,
    RelationshipType,
)


def test_environment_creation():
    """Test Environment object creation"""
    env = Environment(name="VDD", value="1.8V")
    assert env.name == "VDD"
    assert env.value == "1.8V"
    print("✓ Environment creation successful")


def test_parameter_creation():
    """Test Parameter object creation"""
    param = Parameter(name="M1:W", value=16.0, min_val=2.0, max_val=50.0)
    assert param.name == "M1:W"
    assert param.value == 16.0
    assert param.min_val == 2.0
    assert param.max_val == 50.0
    print("✓ Parameter creation successful")


def test_target_creation():
    """Test Target object creation"""
    target = Target(
        metric="DC_GAIN", value=60.0, weight=1.0, mode=TargetMode.Min, unit="dB"
    )
    assert target.metric == "DC_GAIN"
    assert target.value == 60.0
    assert target.weight == 1.0
    assert target.mode == TargetMode.Min
    assert target.unit == "dB"
    print("✓ Target creation successful")


def test_spice_test_creation():
    """Test SpiceTest object creation"""
    env = Environment(name="VDD", value="1.8V")
    test = Test(
        "AC_Analysis",
        [env],
        ".ac dec 100 0.1 1G",
        "AC analysis test",
    )
    assert test.name == "AC_Analysis"
    assert test.spice_code == ".ac dec 100 0.1 1G"
    assert test.description == "AC analysis test"
    print("✓ SpiceTest creation successful")


def test_optimizer_creation():
    """Test Optimizer object creation"""
    optimizer = Optimizer(
        circuit="OpAmp_tb.sch",
        template="test/template",
        solver="newton",
        max_iter=100,
        precision=1e-6,
        verbose=True,
    )
    assert optimizer.circuit == "OpAmp_tb.sch"
    assert optimizer.template == "test/template"
    assert optimizer.solver == "newton"
    assert optimizer.max_iter == 100
    assert optimizer.precision == 1e-6
    assert optimizer.verbose == True
    print("✓ Optimizer creation successful")


def test_optimize_call():
    """Test calling optimize method"""
    print("\nTesting optimize() call...")

    # Create minimal test objects
    param = Parameter(name="M1_W", value=16.0, min_val=2.0, max_val=50.0)
    param2 = Parameter(name="M2_W", value=16.0, min_val=2.0, max_val=50.0)
    env = Environment(name="VDD", value="1.8V")
    test = Test("test", [env], ".dc V1 0 1 0.1", "DC analysis test")
    target = Target(metric="DC_GAIN", value=60.0, weight=1.0, mode=TargetMode.Min, unit="dB")

    optimizer = Optimizer(circuit="OpAmp_tb.sch", template="test/template")

    constraint1 = ParameterConstraint(
        param,
        [param2],
        "M2_W",  # M1_W = M2_W
        RelationshipType.Equals,
        "Match differential pair transistors",
    )

    print("About to call optimize...")
    try:
        result = optimizer.optimize([param, param2], [test], [target], [constraint1])
        print(f"✓ Optimization completed: {result}")
        assert result is not None
    except Exception as e:
        print(f"Exception caught: {e}")
        import traceback

        traceback.print_exc()
        raise


if __name__ == "__main__":
    pytest.main([__file__, "-v", "-s"])

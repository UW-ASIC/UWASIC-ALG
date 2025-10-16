# UWASIC Optimizer

Circuit parameter optimization library for XSchem analog design workflows using ngspice.
Vec<Spice> Handles

## Known Problems

- For multi test setups, the results are incoherent

## Development

### Prerequisites

```bash
# Option 1: nixshell
nix-shell
# Option 2: download xschem and ngspice
```

```bash
maturin develop # Generate the python library
pytest test/ -v # Run Tests
python examples/optimizer.py # Run example
```

## Python-Side Usage

### Parameter Naming

Parameters must follow the format: `COMPONENT_PARAMETER`

- `M1_W` → alters width of transistor `M1`
- `M1_L` → alters length of transistor `M1`
- `R1` → alters value of resistor `R1`

The library automatically maps parameter names to ngspice component parameters:

- Suffix `_W` → transistor width (uses `@component[w]` syntax)
- Suffix `_L` → transistor length (uses `@component[l]` syntax)
- Suffix `_M` → multiplier (uses `@component[m]` syntax)
- No suffix → component value (for R, C, etc.)

### Control Blocks & Vector Naming

The library automatically handles `.control` and `.endc` blocks:

```python
# Target definition
Target(metric="GBW", ...)

spice_code="""
.ac dec 50 1 1G
.control
run
meas ac gbw_val WHEN vdb(vout)=0 CROSS=1
.endc
"""
```

### Example Structure

```
template/
├── OpAmp.sch        # Schematic
├── OpAmp.sym        # Symbol
└── OpAmp_tb.sch     # Testbench (required)
test_flow.py
```

The optimizer:

1. Generates netlist from `OpAmp_tb.sch` using xschem
2. Loads circuit into ngspice
3. Alters parameters for each iteration
4. Runs tests and extracts metrics
5. Computes cost and optimizes

### Constraints

```python
from uwasic_optimizer import ParameterConstraint, RelationshipType

constraints = [
    ParameterConstraint(
        target_param=M1_W,
        source_params=[M2_W],
        expression="XM2_W",  # M1_W = M2_W
        relationship=RelationshipType.Equals,
        description="Match differential pair widths",
    ),
]
```

Every parameter on the right hand side must be defined in `source_params`, you can use any valid mathetmatical expression in expression. The python side will tell you if you are missing any parameters or wrote an incorrect expression.

Supported relationships:

- `Equals`: target = expression
- `GreaterThan`: target > expression
- `GreaterThanOrEqual`: target ≥ expression
- `LessThan`: target < expression
- `LessThanOrEqual`: target ≤ expression

#### Publish to PyPi

```bash
git tag v0.0.* && git push origin v0.0.*
```

1. Add PYPI_API_TOKEN secret to your GitHub repository settings
2. Test by pushing a tag: git tag v0.1.0 && git push origin v0.1.0
3. Note: Users will still need xschem installed separately on their systems

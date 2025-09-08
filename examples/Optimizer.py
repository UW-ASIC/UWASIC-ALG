import os
import sys
sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from xschemoptimizer import optimize_circuit

# OpAmp transistor initial sizing (in um)
INITIAL_PARAMS = {
    "M1": {"W": "10",  "L": "0.5"},
    "M2": {"W": "10",  "L": "0.5"},
    "M3": {"W": "5",   "L": "0.5"},
    "M4": {"W": "5",   "L": "0.5"},
    "M5": {"W": "20",  "L": "0.5"},
    "M6": {"W": "15",  "L": "0.5"}
}

# Op-amp tests optimized for TIA application requirements
TESTS = {
    "dc_characteristics": {
        "V1": "DC 0.9V",
        "V3": "DC 0V AC 1V", 
        "V5": "DC 0V",
        "spice": """
.ac dec 50 1 1G
.control
run
let dc_gain_val = vdb(vout)[0]
echo 'DC_GAIN:' $&dc_gain_val
* Find gain-bandwidth product (GBW)
let gbw_freq = 0
let dc_gain_linear = db(vout)[0]
let target_gain = dc_gain_linear - 3
let i = 1
while i < length(vdb(vout))
  if vdb(vout)[i] <= target_gain
    let gbw_freq = frequency[i-1]
    break
  end
  let i = i + 1
end
if gbw_freq = 0
  let gbw_freq = frequency[length(frequency)-1]
end
echo 'GBW:' $&gbw_freq
* Find unity gain frequency for phase margin calculation
let unity_freq = 0
let i = 0
while i < length(vdb(vout))
  if vdb(vout)[i] <= 0
    let unity_freq = frequency[i]
    break
  end
  let i = i + 1
end
echo 'UNITY_FREQ:' $&unity_freq
.endc
"""
    },
    "input_offset_and_bias": {
        "V1": "DC 0.9V",
        "V3": "DC 0.9V",
        "V5": "DC 0.9V",
        "spice": """
.op
.control
run
* Input bias current (critical for TIA - photodiode loading)
let ibias_pos = i(v3_vplus_input)
let ibias_neg = -i(v5_vminus_input)
let ibias_avg = (abs(ibias_pos) + abs(ibias_neg))/2
echo 'INPUT_BIAS_CURRENT:' $&ibias_avg
* Input offset voltage
let vos = v(vplus) - v(vminus)
echo 'INPUT_OFFSET:' $&vos
.endc
"""
    },
    "slew_rate_analysis": {
        "V1": "DC 0.9V",
        "V3": "PULSE(0.89V 0.91V 1u 10n 10n 10u 20u)",
        "V5": "DC 0.9V",
        "spice": """
.tran 2n 15u
.control
run
let vout_deriv = deriv(v(vout))
let max_slew = maximum(vout_deriv)
let min_slew = minimum(vout_deriv)
let slew_pos = max_slew
let slew_neg = -min_slew
echo 'SLEW_RATE_POS:' $&slew_pos
echo 'SLEW_RATE_NEG:' $&slew_neg
.endc
"""
    },
    "noise_analysis": {
        "V1": "DC 0.9V",
        "V3": "DC 0V",
        "V5": "DC 0V",
        "spice": """
.noise v(vout) v5_vminus_input dec 30 1 1G
.control
run
* Input-referred voltage noise density at 1kHz (critical for TIA SNR)
let noise_1khz_idx = 0
let target_freq = 1000
let i = 0
while i < length(frequency)
  if frequency[i] >= target_freq
    let noise_1khz_idx = i
    break
  end
  let i = i + 1
end
let voltage_noise_1khz = sqrt(inoise_spectrum[noise_1khz_idx])
echo 'VOLTAGE_NOISE_1KHZ:' $&voltage_noise_1khz
* Low frequency (1Hz) noise for 1/f noise assessment
let noise_1hz_idx = 0
let i = 0
while i < length(frequency)
  if frequency[i] >= 1
    let noise_1hz_idx = i
    break
  end
  let i = i + 1
end
let voltage_noise_1hz = sqrt(inoise_spectrum[noise_1hz_idx])
echo 'VOLTAGE_NOISE_1HZ:' $&voltage_noise_1hz
.endc
"""
    },
    "input_capacitance": {
        "V1": "DC 0.9V", 
        "V3": "DC 0V AC 1V",
        "V5": "DC 0V",
        "spice": """
.ac dec 20 1 100k
.control
run
* Calculate input capacitance (affects TIA bandwidth when connected to photodiode)
let input_current = i(v3_vplus_input)
let input_cap = imag(input_current)/(2*pi*frequency[0])
echo 'INPUT_CAP:' $&input_cap
.endc
"""
    },
    "power_consumption": {
        "V1": "DC 0.9V",
        "V3": "DC 0.9V",
        "V5": "DC 0.9V",
        "spice": """
.op
.control
run
let power_val = v(vdd)*(-i(V2))
echo 'POWER:' $&power_val
.endc
"""
    },
    "area_calculation": {
        "V1": "DC 0.9V",
        "V3": "DC 0.9V",
        "V5": "DC 0.9V", 
        "spice": """
.op
.control
run
* Calculate total transistor area
let area_m1 = @M1[W] * @M1[L]
let area_m2 = @M2[W] * @M2[L] 
let area_m3 = @M3[W] * @M3[L]
let area_m4 = @M4[W] * @M4[L]
let area_m5 = @M5[W] * @M5[L]
let area_m6 = @M6[W] * @M6[L]
let total_area = area_m1 + area_m2 + area_m3 + area_m4 + area_m5 + area_m6
echo 'TOTAL_AREA:' $&total_area
.endc
"""
    }
}

# Op-amp targets optimized for TIA application
TARGETS = [
    # MAXIMIZE: DC gain (higher = better closed-loop precision in TIA)
    {"metric": "DC_GAIN", "UNIT": "dB", "target_value": 70.0, "weight": 2.0, "constraint_type": "min"},
    
    # MAXIMIZE: Gain-bandwidth product (higher = better TIA bandwidth)
    {"metric": "GBW", "UNIT": "Hz", "target_value": 100e6, "weight": 2.5, "constraint_type": "min"},
    
    # MAXIMIZE: Slew rate (faster = better TIA transient response)
    {"metric": "SLEW_RATE_POS", "UNIT": "V/s", "target_value": 50e6, "weight": 2.0, "constraint_type": "min"},
    {"metric": "SLEW_RATE_NEG", "UNIT": "V/s", "target_value": 50e6, "weight": 2.0, "constraint_type": "min"},
    
    # MINIMIZE: Input bias current (lower = less photodiode loading)
    {"metric": "INPUT_BIAS_CURRENT", "UNIT": "A", "target_value": 100e-12, "weight": 1.8, "constraint_type": "max"},
    
    # MINIMIZE: Input capacitance (lower = higher TIA bandwidth)
    {"metric": "INPUT_CAP", "UNIT": "F", "target_value": 200e-15, "weight": 1.5, "constraint_type": "max"},
    
    # MINIMIZE: Input voltage noise (lower = better TIA SNR)
    {"metric": "VOLTAGE_NOISE_1KHZ", "UNIT": "V/sqrt(Hz)", "target_value": 5e-9, "weight": 1.8, "constraint_type": "max"},
    
    # MINIMIZE: 1/f noise (lower = better low-frequency performance)
    {"metric": "VOLTAGE_NOISE_1HZ", "UNIT": "V/sqrt(Hz)", "target_value": 50e-9, "weight": 1.2, "constraint_type": "max"},
    
    # MINIMIZE: Input offset (lower = better TIA accuracy)
    {"metric": "INPUT_OFFSET", "UNIT": "V", "target_value": 5e-3, "weight": 1.0, "constraint_type": "max"},
    
    # MINIMIZE: Power consumption
    {"metric": "POWER", "UNIT": "W", "target_value": 5e-6, "weight": 1.2, "constraint_type": "max"},
    
    # MINIMIZE: Area (smaller = lower parasitics)
    {"metric": "TOTAL_AREA", "UNIT": "um2", "target_value": 400.0, "weight": 1.0, "constraint_type": "max"}
]

BOUNDS = [
    # Width bounds - minimum 0.42um for SKY130
    {"component": "M1", "parameter": "W", "min_value": 0.42, "max_value": 40.0},   
    {"component": "M2", "parameter": "W", "min_value": 0.42, "max_value": 40.0},   
    {"component": "M3", "parameter": "W", "min_value": 0.42, "max_value": 15.0},   
    {"component": "M4", "parameter": "W", "min_value": 0.42, "max_value": 15.0},   
    {"component": "M5", "parameter": "W", "min_value": 0.42, "max_value": 80.0},  
    {"component": "M6", "parameter": "W", "min_value": 0.42, "max_value": 60.0},  
    
    # Length bounds - minimum 0.15um for 1.8V SKY130 devices
    {"component": "M1", "parameter": "L", "min_value": 0.15, "max_value": 2.0},    
    {"component": "M2", "parameter": "L", "min_value": 0.15, "max_value": 2.0},    
    {"component": "M3", "parameter": "L", "min_value": 0.15, "max_value": 3.0},    
    {"component": "M4", "parameter": "L", "min_value": 0.15, "max_value": 3.0},    
    {"component": "M5", "parameter": "L", "min_value": 0.15, "max_value": 1.0},   
    {"component": "M6", "parameter": "L", "min_value": 0.15, "max_value": 1.0}    
]

if __name__ == "__main__":
    print("Starting OpAmp optimization...")
    
    result = optimize_circuit(
        "OpAmp", 
        INITIAL_PARAMS, 
        TESTS, 
        TARGETS, 
        BOUNDS,
        template_dir="template", 
        max_iterations=5, 
        target_precision=0.90,
        solver_type="auto",  # Use automatic solver selection
        verbose=False
    )
    
    print("\n=== OPTIMIZATION RESULTS ===")
    total_area = 0.0
    for component, params in result.items():
        w = float(params.get("W", 0))
        l = float(params.get("L", 0))
        area = w * l
        total_area += area
        print(f"{component}: W={w:.3f}um, L={l:.3f}um (Area: {area:.1f}um²)")
    
    print(f"\nTotal Area: {total_area:.1f}um²")
    print("Optimization completed successfully!")

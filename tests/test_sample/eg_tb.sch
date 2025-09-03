v {xschem version=3.4.4 file_version=1.2
}
G {}
K {}
V {}
S {}
E {}
N 0 90 0 160 { lab=#net1 }
N -60 130 -60 160 { lab=#net2 }
N -150 50 -130 50 { lab=V+ }
N -150 -50 -130 -50 { lab=V- }
N 140 0 160 0 { lab=vout }
N -260 0 -210 0 { lab=#net3 }
N -210 0 -210 50 { lab=#net3 }
N -210 -50 -210 0 { lab=#net3 }
C {eg.sym} 20 0 0 0 { name=x1 }
C {devices/vsource.sym} -290 0 1 0 {
name=V1
value="DC 0.9V"
savecurrent=false
}
C {devices/code_shown.sym} 240 120 0 0 {
name=s1
only_toplevel=false
value=".ac dec 100 0.1 1G
.control
run
let dc_gain_val = vdb(vout)[0]
echo 'DC_GAIN:' $&dc_gain_val
.endc
.ac dec 100 0.1 1G
.control
run
let unity_gain_freq = vecmax(frequency)
echo 'UNITY_GAIN_BW:' $&unity_gain_freq
.endc
.op
.control
run
let power_consumption = vdd#branch * 1.8
echo 'POWER:' $&power_consumption
.endc"
=
<=
.endc
            ),
        ];dc_gain_val
.endc
.ac dec 100 0.1 1G
.control
run
let unity_gain_freq="0
let i = 0
while i < length(vdb(vout))
  if vdb(vout)[i] <= 0
    let unity_gain_freq = frequency[i]
    break
  end
  let i = i + 1
end
echo 'UNITY_GAIN_BW:'         // Define target metrics with SPICE code that will be injected into the testbench
        let target_metrics = vec![
            TargetMetric::new(
                DC_GAIN, 
                40.0, // Target: 40 dB DC gain
                .ac"
.endc
            ),
        ];unity_gain_freq
.endc
.op
.control
run
op
let power_consumption="vdd#branch * 1.8
echo 'POWER:'         // Define target metrics with SPICE code that will be injected into the testbench
        let target_metrics = vec![
            TargetMetric::new(
                DC_GAIN, 
                40.0, // Target: 40 dB DC gain
                .ac"
unity_gain_freq="0
let i = 0
while i < length(vdb(vout))
  if vdb(vout)[i] <= 0
    let unity_gain_freq = frequency[i]
    break
  end
  let i = i + 1
end
echo 'UNITY_GAIN_BW:'         // Define target metrics with SPICE code that will be injected into the testbench
        let target_metrics = vec![
            TargetMetric::new(
                DC_GAIN, 
                40.0, // Target: 40 dB DC gain
                .ac"
power_consumption="vdd#branch * 1.8
echo 'POWER:'         // Define target metrics with SPICE code that will be injected into the testbench
        let target_metrics = vec![
            TargetMetric::new(
                DC_GAIN, 
                40.0, // Target: 40 dB DC gain
                .ac"
}
C {sky130_fd_pr/corner.sym} 300 -100 0 0 {
name=CORNER
only_toplevel=false
corner=tt
}
C {devices/lab_pin.sym} 160 0 1 0 {
name=p1
sig_type=std_logic
lab=vout
}
C {devices/lab_pin.sym} -150 50 3 0 {
name=p2
sig_type=std_logic
lab=V+
}
C {devices/lab_pin.sym} -150 -50 3 0 {
name=p3
sig_type=std_logic
lab=V-
}
C {devices/vsource.sym} 30 -90 3 0 {
name=V2
value="DC 1.8V"
savecurrent=false
}
C {devices/gnd.sym} 60 -90 3 0 { name=l6 lab=GND }
C {devices/lab_pin.sym} 0 -90 1 0 {
name=p4
sig_type=std_logic
lab=vdd
}
C {devices/vsource.sym} -30 160 3 0 {
name=V4
value="DC 0.7V"
savecurrent=false
}
C {devices/capa.sym} 140 30 0 0 {
name=C1
m=1
value=5p
footprint=1206
device="ceramic capacitor"
}
C {devices/gnd.sym} 140 60 0 0 { name=l1 lab=GND }
C {devices/vsource.sym} -180 50 1 0 {
name=V3
value="DC 0V AC 1mV"
savecurrent=false
}
C {devices/vsource.sym} -180 -50 1 0 {
name=V5
value="DC 0V AC 1mV"
savecurrent=false
}

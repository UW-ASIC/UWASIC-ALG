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
C {temp_schemsym_94ef86f7072644e59cf390ca033c7194.sym} 20 0 0 0 { name=x1 }
C {devices/vsource.sym} -290 0 1 0 {
name=V1
value="DC 0.9V"
savecurrent=false
}
C {devices/code_shown.sym} 240 120 0 0 {
name=s1
only_toplevel=false
value="
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
"
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
value="DC 0V"
savecurrent=false
}
C {devices/vsource.sym} -180 -50 1 0 {
name=V5
value="DC 0V"
savecurrent=false
}

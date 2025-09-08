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
C {temp_schemsym_6574d04c206f4f3d97733319d317abdb.sym} 20 0 0 0 { name=x1 }
C {devices/vsource.sym} -290 0 1 0 {
name=V1
value="DC 0.9V"
savecurrent=false
}
C {devices/code_shown.sym} 240 120 0 0 {
name=s1
only_toplevel=false
value="
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
value="DC 0V AC 1V"
savecurrent=false
}
C {devices/vsource.sym} -180 -50 1 0 {
name=V5
value="DC 0V"
savecurrent=false
}

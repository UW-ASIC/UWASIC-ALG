v {xschem version=3.4.4 file_version=1.2
}
G {}
K {}
V {}
S {}
E {}
N 0 90 0 160 {
lab=#net1}
N -60 130 -60 160 {
lab=#net2}
N -150 50 -130 50 {
lab=V+}
N -150 -50 -130 -50 {
lab=V-}
N 140 -0 160 0 {
lab=vout}
N -260 0 -210 0 {
lab=#net3}
N -210 0 -210 50 {
lab=#net3}
N -210 -50 -210 0 {
lab=#net3}
N -370 0 -320 0 {}
C {OpAmp.sym} 20 0 0 0 {name=x1}
C {devices/vsource.sym} -290 0 1 0 {name=V1 value="DC 0.9V" savecurrent=false}
C {devices/code_shown.sym} 240 120 0 0 {name=s1 only_toplevel=false value=""}
C {sky130_fd_pr/corner.sym} 300 -100 0 0 {name=CORNER only_toplevel=false corner=tt}
C {devices/lab_pin.sym} 160 0 1 0 {name=p1 sig_type=std_logic lab=vout}
C {devices/lab_pin.sym} -150 50 3 0 {name=p2 sig_type=std_logic lab=V+}
C {devices/lab_pin.sym} -150 -50 3 0 {name=p3 sig_type=std_logic lab=V-}
C {devices/vsource.sym} 30 -90 3 0 {name=V2 value="DC 1.8V" savecurrent=false}
C {devices/gnd.sym} 60 -90 3 0 {name=l6 lab=GND}
C {devices/lab_pin.sym} 0 -90 1 0 {name=p4 sig_type=std_logic lab=vdd}
C {devices/vsource.sym} -30 160 3 0 {name=V4 value="DC 0.7V" savecurrent=false}
C {devices/capa.sym} 140 30 0 0 {name=C1
m=1
value=5p
footprint=1206
device="ceramic capacitor"}
C {devices/gnd.sym} 140 60 0 0 {name=l1 lab=GND}
C {devices/vsource.sym} -180 50 1 0 {name=V3 value="DC 0V AC 1mV" savecurrent=false}
C {devices/vsource.sym} -180 -50 1 0 {name=V5 value="DC 0V AC 1mV" savecurrent=false}
C {devices/gnd.sym} -370 0 0 0 {name=l2 lab=GND}

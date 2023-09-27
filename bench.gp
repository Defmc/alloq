set datafile separator comma
set autoscale fix
set ylabel "nanosseconds"
set xlabel "n. of operations"
set terminal pngcairo size 3702,2048
set xzeroaxis
set output "gp_out.png"

la_path = "linear_allocation.csv"
ld_path = "linear_deallocation.csv"
rd_path = "reverse_deallocation.csv"
vp_path = "vector_pushing.csv"
rt_path = "reset.csv"

set multiplot layout 3,2 columns

set title "linear_allocation"
plot la_path using 1:2 title "bump" with lines linewidth 3, \
     la_path using 1:3 title "debump" with lines linewidth 3, \
     la_path using 1:4 title "pool" with lines linewidth 3
   
set title "linear_deallocation"
plot ld_path using 1:2 title "bump" with lines linewidth 3, \
     ld_path using 1:3 title "debump" with lines linewidth 3, \
     ld_path using 1:4 title "pool" with lines linewidth 3
   
set title "reverse_deallocation"
plot rd_path using 1:2 title "bump" with lines linewidth 3, \
     rd_path using 1:3 title "debump" with lines linewidth 3, \
     rd_path using 1:4 title "pool" with lines linewidth 3
   
   
set title "vector_pushing"
plot vp_path using 1:2 title "bump" with lines linewidth 3, \
     vp_path using 1:3 title "debump" with lines linewidth 3, \
     vp_path using 1:4 title "pool" with lines linewidth 3
   
set title "reset"
plot rt_path using 1:2 title "bump" with lines linewidth 3, \
     rt_path using 1:3 title "debump" with lines linewidth 3, \
     rt_path using 1:4 title "pool" with lines linewidth 3

unset multiplot

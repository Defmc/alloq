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

sources = system("ls *.csv")

set multiplot layout 3,2 columns
do for [source in sources] {
    set title source
    plot source using 1:2 title "bump" with lines linewidth 3, \
         source using 1:3 title "debump" with lines linewidth 3, \
         source using 1:4 title "pool" with lines linewidth 3
}

unset multiplot

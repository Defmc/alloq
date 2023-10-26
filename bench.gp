set datafile separator comma
set autoscale fix
set ylabel "nanosseconds"
set xlabel "n. of operations"
set terminal pngcairo size 3702,2048
set xzeroaxis
set output "gp_out.png"

sources = system("ls *.csv")

set multiplot layout 3,2 columns
do for [source in sources] {
    set title source
    plot source using 1:2 title "first" with lines linewidth 3, \
         source using 1:3 title "best" with lines linewidth 3, \
         source using 1:4 title "dump" with lines linewidth 3, \
         source using 1:5 title "dedump" with lines linewidth 3, \
         source using 1:6 title "pool" with lines linewidth 3, \
         source using 1:7 title "statiq" with lines linewidth 3
}

unset multiplot

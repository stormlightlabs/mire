#!/usr/bin/env perl
# Rewrite each SGR sequence in the ANSI text on stdin, or in the files named,
# so that it resets everything and then sets the attributes still on.
# Usage: .github/storm/sgr.pl [file...]
#
# freeze 0.2.2 ignores the SGR codes that reset one attribute, such as 39 and
# 49 for the default colors, so a color runs on past its field. shot.sh
# renders from this output. Underline color (58 and 59) is dropped, since
# freeze does not draw it. Colon forms, such as tmux's 4:3 for a curly
# underline and 38:2::r:g:b, are read and written back with ';'.
use strict;
use warnings;

my %on;

sub sgr {
  my ($params) = @_;
  my @p = split /;/, $params, -1;
  @p = (0) unless @p;
  while (@p) {
    my $param = shift @p;
    if ($param =~ /:/) {
      colon(split /:/, $param, -1);
      next;
    }
    my $c = $param || 0;
    if ($c == 0) { %on = () }
    elsif ($c >= 1 && $c <= 9) { $on{$c} = $c }
    elsif ($c == 22) { delete @on{1, 2} }
    elsif ($c == 25) { delete @on{5, 6} }
    elsif ($c >= 23 && $c <= 29 && $c != 26) { delete $on{$c - 20} }
    elsif (($c >= 30 && $c <= 37) || ($c >= 90 && $c <= 97)) { $on{fg} = $c }
    elsif ($c == 39) { delete $on{fg} }
    elsif (($c >= 40 && $c <= 47) || ($c >= 100 && $c <= 107)) { $on{bg} = $c }
    elsif ($c == 49) { delete $on{bg} }
    elsif ($c == 38 || $c == 48 || $c == 58) {
      my $mode = shift(@p) || 0;
      my @args = $mode == 5 ? (shift(@p) || 0) : $mode == 2 ? (map { shift(@p) || 0 } 1 .. 3) : ();
      color($c, $mode, @args);
    }
  }
  return "\e[" . join(";", 0, grep { defined } @on{1 .. 9, "fg", "bg"}) . "m";
}

# One parameter written with ':' sub-parameters. 4:0 ends underline and any
# other 4:N sets it. 38, 48 and 58 take 5:n, 2:r:g:b, or 2:id:r:g:b, where id
# names a color space and is often left empty. Other colon forms are dropped.
sub colon {
  my ($c, @sub) = map { $_ || 0 } @_;
  if ($c == 4) {
    if (@sub && $sub[0] == 0) { delete $on{4} } else { $on{4} = 4 }
  }
  elsif ($c == 38 || $c == 48 || $c == 58) {
    my $mode = shift(@sub) || 0;
    shift @sub if $mode == 2 && @sub >= 4;
    my @args = $mode == 5 ? ($sub[0] || 0) : $mode == 2 ? (map { $sub[$_] || 0 } 0 .. 2) : ();
    color($c, $mode, @args);
  }
}

# Record a 38 or 48 color; 58, the underline color, is dropped.
sub color {
  my ($c, $mode, @args) = @_;
  return if $c == 58 || !@args;
  $on{$c == 38 ? "fg" : "bg"} = join ";", $c, $mode, @args;
}

while (my $line = <>) {
  $line =~ s/\e\[([0-9;:]*)m/sgr($1)/ge;
  print $line;
}

# The stand-in of `whizbang play --no-open --stdin-quit --port 0` for
# scripts/tests/test-install-check.sh: it listens on a free port of 127.0.0.1, prints the
# address with a token, answers `/` with a page under a content security policy and
# `/api/hello` with the version to the token alone, and ends when its input does, removing
# the play.json it wrote. Started as `perl play-stand-in.pl <version> <kind>`; the kind
# `play-stays` does not end when its input does.
use strict;
use warnings;
use IO::Socket::INET;
use IO::Select;

my ($version, $kind) = @ARGV;
my $token = 'ab' x 32;
my $listen = IO::Socket::INET->new(LocalAddr => '127.0.0.1', LocalPort => 0, Listen => 8, ReuseAddr => 1)
  or die "cannot listen: $!\n";
my $port = $listen->sockport;
open(my $file, '>', 'store/play.json') or die "cannot write play.json: $!\n";
print $file "{}\n";
close $file;
$| = 1;
print "http://127.0.0.1:$port/#k=$token\n";
my $select = IO::Select->new($listen, \*STDIN);
while (1) {
  for my $ready ($select->can_read) {
    if ($ready == $listen) {
      my $client = $listen->accept or next;
      my $head = '';
      while (my $line = <$client>) {
        $head .= $line;
        last if $line eq "\r\n";
      }
      my ($path) = $head =~ m{^GET (\S+) };
      my ($status, $type, $body, $more) = (404, 'application/json', '{"error":"no such route"}', '');
      if (defined $path && $path eq '/') {
        ($status, $type, $body) = (200, 'text/html', "<!doctype html>\n");
        $more = "Content-Security-Policy: default-src 'self'\r\n";
      } elsif (defined $path && $path eq '/api/hello') {
        if ($head =~ /^Authorization: Bearer \Q$token\E\r$/mi) {
          ($status, $body) = (200, qq({"name":"whizbang","version":"$version"}));
        } else {
          ($status, $body) = (401, '{"error":"run whizbang play again"}');
        }
      }
      print $client "HTTP/1.1 $status X\r\nContent-Type: $type\r\nContent-Length: " . length($body)
        . "\r\nConnection: close\r\n$more\r\n$body";
      close $client;
    } else {
      my $read = sysread(STDIN, my $buffer, 4096);
      next if $read;
      $select->remove(\*STDIN);
      if ($kind ne 'play-stays') {
        unlink 'store/play.json';
        exit 0;
      }
      sleep 60;
      exit 0;
    }
  }
}

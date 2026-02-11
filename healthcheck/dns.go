// Copyright 2012 Google Inc. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Author: jsing@google.com (Joel Sing)

// DNS healthcheck implementation.

package healthcheck

import (
	"fmt"
	"net"
	"strings"
	"time"

	"github.com/google/seesaw/common/seesaw"

	"github.com/miekg/dns"
)

const (
	defaultDNSTimeout = 3 * time.Second
)

// DNSType returns the dnsType that corresponds with the given name.
func DNSType(name string) (uint16, error) {
	dt, ok := dns.StringToType[strings.ToUpper(name)]
	if !ok {
		return 0, fmt.Errorf("unknown DNS type %q", name)
	}
	return dt, nil
}

// DNSChecker contains configuration specific to a DNS healthcheck.
type DNSChecker struct {
	Target
	Question dns.Question
	Answer   string
	UseTCP   bool // Use TCP instead of UDP for DNS queries (e.g., for large responses).
}

// NewDNSChecker returns an initialised DNSChecker.
func NewDNSChecker(ip net.IP, port int) *DNSChecker {
	return &DNSChecker{
		Target: Target{
			IP:    ip,
			Port:  port,
			Proto: seesaw.IPProtoUDP,
		},
		Question: dns.Question{
			Qclass: dns.ClassINET,
			Qtype:  dns.TypeA,
		},
	}
}

// tcpNetwork returns the TCP network name for the DNS checker's target.
func (hc *DNSChecker) tcpNetwork() string {
	if hc.IP.To4() != nil {
		return "tcp4"
	}
	return "tcp6"
}

func questionToString(q dns.Question) string {
	return fmt.Sprintf("%s %s %s", q.Name, dns.Class(q.Qclass), dns.Type(q.Qtype))
}

// String returns the string representation of a DNS healthcheck.
func (hc *DNSChecker) String() string {
	return fmt.Sprintf("DNS %s %s", questionToString(hc.Question), hc.Target)
}

// Check executes a DNS healthcheck.
func (hc *DNSChecker) Check(timeout time.Duration) *Result {
	if !strings.HasSuffix(hc.Question.Name, ".") {
		hc.Question.Name += "."
	}

	msg := fmt.Sprintf("DNS %s query to port %d", questionToString(hc.Question), hc.Port)
	start := time.Now()
	if timeout == time.Duration(0) {
		timeout = defaultDNSTimeout
	}
	deadline := start.Add(timeout)

	var aIP net.IP
	switch hc.Question.Qtype {
	case dns.TypeA:
		if aIP = net.ParseIP(hc.Answer); aIP == nil || aIP.To4() == nil {
			msg = fmt.Sprintf("%s; %q is not a valid IPv4 address", msg, hc.Answer)
			return complete(start, msg, false, nil)
		}
	case dns.TypeAAAA:
		if aIP = net.ParseIP(hc.Answer); aIP == nil {
			msg = fmt.Sprintf("%s; %q is not a valid IPv6 address", msg, hc.Answer)
			return complete(start, msg, false, nil)
		}
	}

	// Build DNS query.
	q := &dns.Msg{
		MsgHdr: dns.MsgHdr{
			Id:               dns.Id(),
			RecursionDesired: true,
		},
		Question: []dns.Question{hc.Question},
	}

	var conn net.Conn
	var err error
	if hc.UseTCP {
		conn, err = dialTCP(hc.tcpNetwork(), hc.addr(), timeout, hc.Mark)
	} else {
		conn, err = dialUDP(hc.network(), hc.addr(), timeout, hc.Mark)
	}
	if err != nil {
		return complete(start, msg, false, err)
	}
	defer conn.Close()

	err = conn.SetDeadline(deadline)
	if err != nil {
		msg = fmt.Sprintf("%s; failed to set deadline", msg)
		return complete(start, msg, false, err)
	}

	dnsConn := &dns.Conn{Conn: conn}
	if err := dnsConn.WriteMsg(q); err != nil {
		msg = fmt.Sprintf("%s; failed to send request", msg)
		return complete(start, msg, false, err)
	}

	r, err := dnsConn.ReadMsg()
	if err != nil {
		msg = fmt.Sprintf("%s; failed to read response", msg)
		return complete(start, msg, false, err)
	}

	// Check reply.
	if !r.Response {
		msg = fmt.Sprintf("%s; not a query response", msg)
		return complete(start, msg, false, nil)
	}
	if rc := r.Rcode; rc != dns.RcodeSuccess {
		msg = fmt.Sprintf("%s; non-zero response code - %d", msg, rc)
		return complete(start, msg, false, nil)
	}
	if len(r.Answer) < 1 {
		msg = fmt.Sprintf("%s; no answers received for query %s", msg, questionToString(hc.Question))
		return complete(start, msg, false, nil)
	}

	// Validate that the response question section matches our query.
	if len(r.Question) > 0 && r.Question[0] != hc.Question {
		msg = fmt.Sprintf("%s; response question mismatch: got %s, want %s",
			msg, questionToString(r.Question[0]), questionToString(hc.Question))
		return complete(start, msg, false, nil)
	}

	// Build a CNAME chain map for following aliases in A/AAAA queries.
	cnameMap := make(map[string]string)
	for _, rr := range r.Answer {
		if cname, ok := rr.(*dns.CNAME); ok {
			cnameMap[cname.Hdr.Name] = cname.Target
		}
	}

	// resolveCNAME follows CNAME chains to find the canonical name for a given name.
	resolveCNAME := func(name string) string {
		seen := make(map[string]bool)
		for {
			target, ok := cnameMap[name]
			if !ok || seen[name] {
				return name
			}
			seen[name] = true
			name = target
		}
	}

	for _, rr := range r.Answer {
		if rr.Header().Class != hc.Question.Qclass {
			continue
		}

		switch rr := rr.(type) {
		case *dns.A:
			// For A queries, follow CNAMEs: check if this record's name
			// is reachable from the question name via CNAME chain.
			if hc.Question.Qtype == dns.TypeA {
				canonical := resolveCNAME(hc.Question.Name)
				if rr.Hdr.Name == canonical && aIP.Equal(rr.A) {
					msg = fmt.Sprintf("%s; received answer %s", msg, rr.A)
					return complete(start, msg, true, err)
				}
			}
		case *dns.AAAA:
			// For AAAA queries, follow CNAMEs similarly.
			if hc.Question.Qtype == dns.TypeAAAA {
				canonical := resolveCNAME(hc.Question.Name)
				if rr.Hdr.Name == canonical && aIP.Equal(rr.AAAA) {
					msg = fmt.Sprintf("%s; received answer %s", msg, rr.AAAA)
					return complete(start, msg, true, err)
				}
			}
		case *dns.CNAME:
			if hc.Question.Qtype == dns.TypeCNAME &&
				rr.Hdr.Name == hc.Question.Name &&
				strings.EqualFold(rr.Target, hc.Answer+".") {
				msg = fmt.Sprintf("%s; received CNAME %s", msg, rr.Target)
				return complete(start, msg, true, err)
			}
		case *dns.NS:
			if hc.Question.Qtype == dns.TypeNS &&
				rr.Hdr.Name == hc.Question.Name &&
				strings.EqualFold(rr.Ns, hc.Answer+".") {
				msg = fmt.Sprintf("%s; received NS %s", msg, rr.Ns)
				return complete(start, msg, true, err)
			}
		case *dns.SOA:
			if hc.Question.Qtype == dns.TypeSOA &&
				rr.Hdr.Name == hc.Question.Name {
				msg = fmt.Sprintf("%s; received SOA %s %s", msg, rr.Ns, rr.Mbox)
				return complete(start, msg, true, err)
			}
		}
	}

	msg = fmt.Sprintf("%s; failed to match answer", msg)
	return complete(start, msg, false, err)
}

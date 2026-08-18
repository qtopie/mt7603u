#!/usr/bin/env bash
set -euo pipefail

# Hardware association verification script for MT7603U driver.
# Usage: ./scripts/test-hardware-assoc.sh [IFNAME] [SSID] [PASSPHRASE]

IFNAME="${1:-wlxb0d59d8c8c7a}"
SSID="${2:-WiFi-Test-AP}"
PASSPHRASE="${3:-12345678}"

echo "================================================================"
echo "⚡ [MT7603U Hardware Association Test] Starting Validation ⚡"
echo "================================================================"
echo "Interface:  $IFNAME"
echo "SSID:       $SSID"
echo "================================================================"

# Check if interface exists
if ! ip link show "$IFNAME" >/dev/null 2>&1; then
    echo "❌ Error: Interface $IFNAME not found!"
    echo "Please insert the device or pass the correct interface name as the first argument."
    exit 1
fi

# Bring interface UP
echo "--> 1. Bringing interface $IFNAME UP..."
if [ -f ~/.pass ]; then
    cat ~/.pass | sudo -S ip link set "$IFNAME" up || true
else
    sudo ip link set "$IFNAME" up || true
fi
sleep 1

# Scan to verify receiving beacons
echo "--> 2. Scanning to see if target AP '$SSID' is visible..."
if [ -f ~/.pass ]; then
    SCAN_RES=$(cat ~/.pass | sudo -S iw dev "$IFNAME" scan 2>/dev/null || true)
else
    SCAN_RES=$(sudo iw dev "$IFNAME" scan 2>/dev/null || true)
fi

AP_FREQ=""
if ! echo "$SCAN_RES" | grep -q "SSID: $SSID"; then
    echo "⚠️ Warning: Target SSID '$SSID' not found in active scan results."
    echo "Proceeding with association attempt anyway, in case of hidden SSID or passive scan..."
else
    echo "✅ Found target AP '$SSID' in scan results!"
    AP_FREQ=$(echo "$SCAN_RES" | awk -v RS='BSS ' -v ssid="$SSID" '$0 ~ ("SSID: " ssid) { if (match($0, /freq: ([0-9]+)/, m)) print m[1] }' | head -n 1)
    if [ -n "$AP_FREQ" ]; then
        echo "--> Target AP frequency: ${AP_FREQ} MHz"
    fi
fi

# Ensure wifi is not soft/hard blocked
if [ -f ~/.pass ]; then
    cat ~/.pass | sudo -S rfkill unblock wifi || true
else
    sudo rfkill unblock wifi || true
fi

# Create temporary wpa_supplicant configuration
CONF_FILE="/tmp/wpa_supplicant_mt7603.conf"
echo "--> 3. Generating wpa_supplicant configuration..."
cat <<EOF > "$CONF_FILE"
ctrl_interface=/var/run/wpa_supplicant
update_config=1
network={
    ssid="$SSID"
    psk="$PASSPHRASE"
    key_mgmt=WPA-PSK
    proto=RSN WPA
    pairwise=CCMP TKIP
    group=CCMP TKIP
$( [ -n "$AP_FREQ" ] && echo "    scan_freq=$AP_FREQ" || true )
$( [ -n "$AP_FREQ" ] && echo "    freq_list=$AP_FREQ" || true )
}
EOF

# Kill any existing wpa_supplicant instance on this interface
echo "--> 4. Clearing existing wpa_supplicant instances..."
if [ -f ~/.pass ]; then
    cat ~/.pass | sudo -S killall wpa_supplicant >/dev/null 2>&1 || true
else
    sudo killall wpa_supplicant >/dev/null 2>&1 || true
fi
sleep 1

# Run wpa_supplicant with debug logs to capture the 4-way handshake
echo "--> 5. Launching wpa_supplicant on $IFNAME..."
SUPPLICANT_LOG="/tmp/wpa_supplicant_mt7603.log"
rm -f "$SUPPLICANT_LOG"

if [ -f ~/.pass ]; then
    cat ~/.pass | sudo -S wpa_supplicant -i "$IFNAME" -c "$CONF_FILE" -d -f "$SUPPLICANT_LOG" &
else
    sudo wpa_supplicant -i "$IFNAME" -c "$CONF_FILE" -d -f "$SUPPLICANT_LOG" &
fi
SUB_PID=$!

sleep 0.5
# Ensure the log file is readable by current user
if [ -f ~/.pass ]; then
    cat ~/.pass | sudo -S chmod 644 "$SUPPLICANT_LOG" || true
    cat ~/.pass | sudo -S chown "$(whoami)" "$SUPPLICANT_LOG" || true
else
    sudo chmod 644 "$SUPPLICANT_LOG" || true
    sudo chown "$(whoami)" "$SUPPLICANT_LOG" || true
fi

echo "wpa_supplicant started (PID: $SUB_PID), logging to $SUPPLICANT_LOG"
echo "Monitoring association and 4-way handshake for up to 15 seconds..."

SUCCESS=0
for i in {1..15}; do
    sleep 1
    if [ ! -f "$SUPPLICANT_LOG" ]; then
        continue
    fi
    
    # Check for successful WPA 4-way handshake completion
    if grep -q "State: COMPLETED" "$SUPPLICANT_LOG" || grep -q "WPA: Key negotiation completed with" "$SUPPLICANT_LOG"; then
        echo "✅ [SUCCESS] 4-Way Handshake Completed! State: COMPLETED reached."
        SUCCESS=1
        break
    fi
    
    # Check if we got associated at the mac80211/driver level
    if grep -q "State: ASSOCIATED" "$SUPPLICANT_LOG"; then
        echo "--> State: ASSOCIATED reached. Negotiating keys (4-way handshake)..."
    elif grep -q "State: ASSOCIATING" "$SUPPLICANT_LOG"; then
        echo "--> State: ASSOCIATING..."
    fi
done

# Clean up wpa_supplicant
echo "--> 6. Cleaning up..."
if [ -f ~/.pass ]; then
    cat ~/.pass | sudo -S kill "$SUB_PID" >/dev/null 2>&1 || true
    cat ~/.pass | sudo -S rm -f "$CONF_FILE" || true
else
    sudo kill "$SUB_PID" >/dev/null 2>&1 || true
    rm -f "$CONF_FILE"
fi

# Print diagnostics if failed
if [ $SUCCESS -eq 0 ]; then
    echo "❌ [FAILURE] Handshake did not complete within timeout."
    echo "--- Last 20 lines of wpa_supplicant log ---"
    if [ -f "$SUPPLICANT_LOG" ]; then
        tail -n 20 "$SUPPLICANT_LOG" || true
    elif [ -f ~/.pass ]; then
        cat ~/.pass | sudo -S tail -n 20 "$SUPPLICANT_LOG" || true
    else
        sudo tail -n 20 "$SUPPLICANT_LOG" || true
    fi
    echo "--- Relevant dmesg logs ---"
    if [ -f ~/.pass ]; then
        cat ~/.pass | sudo -S dmesg | grep -E "(mt7603u|mac80211)" | tail -n 25 || true
    else
        sudo dmesg | grep -E "(mt7603u|mac80211)" | tail -n 25 || true
    fi
    exit 1
else
    echo "🎉 [SUCCESS] MT7603U driver hardware association & 4-way handshake validated successfully!"
    exit 0
fi

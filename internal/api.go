package internal

import (
	"encoding/json"
	"net/http"
	"os/exec"
	"time"
)

// HandleStatus aggregates all metrics into one JSON response.
func HandleStatus(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"hostname":    hostname(),
		"cpu":         getCPU(),
		"gpu":         getGPU(),
		"memory":      getMemory(),
		"storage":     getStorage(),
		"disk_health": getDiskHealth(),
		"network":     getNetwork(),
		"docker":      getDocker(),
		"vms":         getVMs(),
		"services":    getServices(),
		"uptime":      getUptime(),
		"timestamp":   time.Now().Format(time.RFC3339),
	})
}

// HandleReboot reboots the system (with confirmation).
func HandleReboot(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "POST required", http.StatusMethodNotAllowed)
		return
	}
	var body struct{ Confirm bool }
	json.NewDecoder(r.Body).Decode(&body)
	if !body.Confirm {
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]string{"error": "confirmation required"})
		return
	}
	exec.Command("/usr/bin/sync").Run()
	go func() {
		time.Sleep(200 * time.Millisecond)
		exec.Command("/bin/systemctl", "reboot", "--force").Run()
	}()
	json.NewEncoder(w).Encode(map[string]string{"status": "rebooting"})
}

// HandleShutdown shuts down the system (with confirmation).
func HandleShutdown(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "POST required", http.StatusMethodNotAllowed)
		return
	}
	var body struct{ Confirm bool }
	json.NewDecoder(r.Body).Decode(&body)
	if !body.Confirm {
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]string{"error": "confirmation required"})
		return
	}
	exec.Command("/usr/bin/sync").Run()
	go func() {
		time.Sleep(200 * time.Millisecond)
		exec.Command("/bin/systemctl", "poweroff", "--force").Run()
	}()
	json.NewEncoder(w).Encode(map[string]string{"status": "shutting down"})
}

// HandleScreenOff turns off the display panel via DPMS.
func HandleScreenOff(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "POST required", http.StatusMethodNotAllowed)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	if err := osWriteFile("/sys/class/drm/card1-DSI-1/dpms", "Off"); err != nil {
		json.NewEncoder(w).Encode(map[string]string{"screen": "error", "error": err.Error()})
		return
	}
	json.NewEncoder(w).Encode(map[string]string{"screen": "off"})
}

// HandleScreenOn turns on the display panel via DPMS.
func HandleScreenOn(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "POST required", http.StatusMethodNotAllowed)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	if err := osWriteFile("/sys/class/drm/card1-DSI-1/dpms", "On"); err != nil {
		json.NewEncoder(w).Encode(map[string]string{"screen": "error", "error": err.Error()})
		return
	}
	json.NewEncoder(w).Encode(map[string]string{"screen": "on"})
}

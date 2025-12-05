import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { Play, Square, Folder, Settings, Mic, Volume2, Trash2, Edit2, Keyboard } from 'lucide-react';
import './App.css';

interface Sound {
  id: string;
  name: string;
  path: string;
  volume: number;
  hotkey?: string;
}

interface AppState {
  sounds: Sound[];
  currentFolder?: string;
  masterVolume: number;
  systemAudioRoutingEnabled: boolean;
}

function App() {
  const [sounds, setSounds] = useState<Sound[]>([]);
  const [masterVolume, setMasterVolume] = useState(1.0);
  const [systemAudioRouting, setSystemAudioRouting] = useState(false);
  const [playingSounds, setPlayingSounds] = useState<Set<string>>(new Set());
  const [editingSound, setEditingSound] = useState<string | null>(null);
  const [editName, setEditName] = useState('');
  const [editHotkey, setEditHotkey] = useState('');
  const [editVolume, setEditVolume] = useState(1.0);
  const [showSettings, setShowSettings] = useState(false);
  const [virtualMicSetup, setVirtualMicSetup] = useState(false);

  useEffect(() => {
    loadState();
    checkVirtualMicStatus();
  }, []);

  const loadState = async () => {
    try {
      const state = await invoke<AppState>('get_state');
      setSounds(state.sounds);
      setMasterVolume(state.masterVolume);
      setSystemAudioRouting(state.systemAudioRoutingEnabled);
    } catch (error) {
      console.error('Failed to load state:', error);
    }
  };

  const checkVirtualMicStatus = async () => {
    try {
      const exists = await invoke<boolean>('check_virtual_mic_exists');
      setVirtualMicSetup(exists);
    } catch (error) {
      console.error('Failed to check virtual mic status:', error);
    }
  };

  const handleLoadFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });

      if (selected) {
        const loadedSounds = await invoke<Sound[]>('load_folder', { folder: selected });
        setSounds(loadedSounds);
      }
    } catch (error) {
      console.error('Failed to load folder:', error);
    }
  };

  const handlePlaySound = async (sound: Sound) => {
    try {
      await invoke('play_sound', { id: sound.id });
      setPlayingSounds(prev => new Set(prev).add(sound.id));
      setTimeout(() => {
        setPlayingSounds(prev => {
          const newSet = new Set(prev);
          newSet.delete(sound.id);
          return newSet;
        });
      }, 500);
    } catch (error) {
      console.error('Failed to play sound:', error);
    }
  };

  const handleStopAll = async () => {
    try {
      await invoke('stop_all_sounds');
      setPlayingSounds(new Set());
    } catch (error) {
      console.error('Failed to stop all sounds:', error);
    }
  };

  const handleMasterVolumeChange = async (volume: number) => {
    setMasterVolume(volume);
    try {
      await invoke('set_master_volume', { volume });
    } catch (error) {
      console.error('Failed to set master volume:', error);
    }
  };

  const handleSetupVirtualMic = async () => {
    try {
      if (virtualMicSetup) {
        // Cleanup/disable virtual mic
        await invoke('cleanup_virtual_microphone');
        setVirtualMicSetup(false);
        alert('Virtual microphone disabled.\n\nYour default audio output has been restored.');
      } else {
        // Setup virtual mic
        const sinkName = await invoke<string>('setup_virtual_microphone');
        setVirtualMicSetup(true);
        alert(`Virtual microphone setup complete!

IMPORTANT: Your system audio output is now set to "${sinkName}"
This means:
- Soundboard audio goes through the virtual mic
- You won't hear sounds unless you monitor the virtual mic
- To hear sounds, keep your headphones on

In Discord:
1. Go to User Settings → Voice & Video
2. Under "Input Device", select "Soundboard Virtual Microphone"
3. Test your mic - you should see activity when playing sounds

The microphone will show as "SoundboardMic" or "Soundboard Virtual Microphone" in Discord.`);
      }
    } catch (error) {
      console.error('Failed to toggle virtual mic:', error);
      alert('Failed to toggle virtual microphone: ' + error);
    }
  };

  const handleToggleSystemAudio = async () => {
    const newState = !systemAudioRouting;
    setSystemAudioRouting(newState);
    try {
      await invoke('toggle_system_audio_routing', { enabled: newState });
    } catch (error) {
      console.error('Failed to toggle system audio routing:', error);
      alert('Failed to toggle system audio routing: ' + error);
    }
  };

  const handleDeleteSound = async (id: string) => {
    try {
      await invoke('remove_sound', { id });
      setSounds(sounds.filter(s => s.id !== id));
    } catch (error) {
      console.error('Failed to delete sound:', error);
    }
  };

  const handleStartEdit = (sound: Sound) => {
    setEditingSound(sound.id);
    setEditName(sound.name);
    setEditHotkey(sound.hotkey || '');
    setEditVolume(sound.volume);
  };

  const handleSaveEdit = async (id: string) => {
    try {
      await invoke('update_sound', {
        id,
        name: editName,
        volume: editVolume,
        hotkey: editHotkey,
      });

      setSounds(sounds.map(s =>
        s.id === id
          ? { ...s, name: editName, volume: editVolume, hotkey: editHotkey || undefined }
          : s
      ));
      setEditingSound(null);
    } catch (error) {
      console.error('Failed to update sound:', error);
      alert('Failed to update sound: ' + error);
    }
  };

  return (
    <div className="app">
      <header className="header">
        <h1>Soundboard</h1>
        <div className="header-controls">
          <button onClick={handleLoadFolder} className="btn btn-primary">
            <Folder size={18} />
            Load Folder
          </button>
          <button onClick={handleStopAll} className="btn btn-danger">
            <Square size={18} />
            Stop All
          </button>
          <button onClick={() => setShowSettings(!showSettings)} className="btn btn-secondary">
            <Settings size={18} />
            Settings
          </button>
        </div>
      </header>

      {showSettings && (
        <div className="settings-panel">
          <h2>Settings</h2>

          <div className="setting-group">
            <label>
              <Volume2 size={18} />
              Master Volume: {Math.round(masterVolume * 100)}%
            </label>
            <input
              type="range"
              min="0"
              max="2"
              step="0.01"
              value={masterVolume}
              onChange={(e) => handleMasterVolumeChange(parseFloat(e.target.value))}
              className="volume-slider"
            />
          </div>

          <div className="setting-group">
            <button
              onClick={handleSetupVirtualMic}
              className={`btn ${virtualMicSetup ? 'btn-success' : 'btn-primary'}`}
            >
              <Mic size={18} />
              {virtualMicSetup ? 'Disable Virtual Mic' : 'Setup Virtual Microphone'}
            </button>
            <p className="setting-help">
              {virtualMicSetup ? (
                <>Virtual mic is active. In Discord, select <strong>"Soundboard Virtual Microphone"</strong> as your input device.</>
              ) : (
                'Creates a virtual microphone that combines your real mic with soundboard audio. Click to enable.'
              )}
            </p>
          </div>

          <div className="setting-group">
            <label className="toggle-label">
              <input
                type="checkbox"
                checked={systemAudioRouting}
                onChange={handleToggleSystemAudio}
              />
              <span>Route System Audio to Mic (YouTube, Spotify, etc.)</span>
            </label>
            <p className="setting-help">
              When enabled, your system audio (browser, music player) will be routed through your microphone.
            </p>
          </div>
        </div>
      )}

      <main className="main-content">
        {sounds.length === 0 ? (
          <div className="empty-state">
            <Folder size={64} />
            <h2>No Sounds Loaded</h2>
            <p>Click "Load Folder" to get started</p>
          </div>
        ) : (
          <div className="sounds-grid">
            {sounds.map((sound) => (
              <div
                key={sound.id}
                className={`sound-card ${playingSounds.has(sound.id) ? 'playing' : ''}`}
              >
                {editingSound === sound.id ? (
                  <div className="sound-edit">
                    <input
                      type="text"
                      value={editName}
                      onChange={(e) => setEditName(e.target.value)}
                      className="edit-input"
                      placeholder="Name"
                    />
                    <input
                      type="text"
                      value={editHotkey}
                      onChange={(e) => setEditHotkey(e.target.value)}
                      className="edit-input"
                      placeholder="Hotkey (e.g., Ctrl+Alt+A)"
                    />
                    <label className="volume-label">
                      Volume: {Math.round(editVolume * 100)}%
                    </label>
                    <input
                      type="range"
                      min="0"
                      max="2"
                      step="0.01"
                      value={editVolume}
                      onChange={(e) => setEditVolume(parseFloat(e.target.value))}
                      className="volume-slider small"
                    />
                    <div className="edit-actions">
                      <button onClick={() => handleSaveEdit(sound.id)} className="btn btn-success btn-sm">
                        Save
                      </button>
                      <button onClick={() => setEditingSound(null)} className="btn btn-secondary btn-sm">
                        Cancel
                      </button>
                    </div>
                  </div>
                ) : (
                  <>
                    <div className="sound-header">
                      <h3 className="sound-name">{sound.name}</h3>
                      <div className="sound-actions">
                        <button
                          onClick={() => handleStartEdit(sound)}
                          className="icon-btn"
                          title="Edit"
                        >
                          <Edit2 size={14} />
                        </button>
                        <button
                          onClick={() => handleDeleteSound(sound.id)}
                          className="icon-btn danger"
                          title="Delete"
                        >
                          <Trash2 size={14} />
                        </button>
                      </div>
                    </div>
                    {sound.hotkey && (
                      <div className="sound-hotkey">
                        <Keyboard size={12} />
                        {sound.hotkey}
                      </div>
                    )}
                    <button
                      onClick={() => handlePlaySound(sound)}
                      className="sound-play-btn"
                    >
                      <Play size={24} />
                      Play
                    </button>
                  </>
                )}
              </div>
            ))}
          </div>
        )}
      </main>
    </div>
  );
}

export default App;

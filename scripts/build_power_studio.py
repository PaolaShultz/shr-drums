#!/usr/bin/env python3
"""Reproducible offline Power Studio pack builder. No audio-device access."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import shutil
import subprocess
import urllib.request

import numpy as np
from scipy import signal
import soundfile as sf

RECIPE = Path(__file__).resolve().parents[1] / 'packs/power-studio.recipe.json'
RATE = 48000
MULD = 'muldjord/MuldjordKit SFZ+WAV-20201018/'
ARCHIVE_URL = 'https://github.com/freepats/muldjordkit/releases/download/2020-10-18/MuldjordKit-SFZ%2BWAV-20201018.7z'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def checked_path(root, relative):
    p = (root / relative).resolve()
    if not p.is_relative_to(root.resolve()):
        raise ValueError('source path escapes root')
    return p


def fetch(root, recipe):
    """Fetch only pinned public inputs; never accept or replace a mismatched file."""
    root.mkdir(parents=True, exist_ok=True)
    def download(url, path, expected):
        if not path.exists():
            path.parent.mkdir(parents=True, exist_ok=True)
            temporary = path.with_suffix(path.suffix + '.download')
            with temporary.open('xb') as out, urllib.request.urlopen(url, timeout=90) as src:
                shutil.copyfileobj(src, out)
            if digest(temporary) != expected:
                temporary.unlink()
                raise ValueError(f'hash mismatch: {url}')
            temporary.rename(path)
        if digest(path) != expected:
            raise ValueError(f'changed source: {path}')
    if not (root / MULD).is_dir():
        archive = root / 'muldjord.7z'
        download(ARCHIVE_URL, archive, recipe['muldjord_archive_sha256'])
        # Only the SHA-verified upstream archive is extracted, into a fresh root.
        extract = root / 'muldjord'
        extract.mkdir(exist_ok=False)
        subprocess.run(['7z', 'x', str(archive), '-o' + str(extract)], check=True)
    for name, expected in recipe['source_hashes'].items():
        if name.startswith('virtuosity/'):
            url = ('https://raw.githubusercontent.com/sfzinstruments/virtuosity_drums/'
                   + recipe['virtuosity_commit'] + '/' + name.removeprefix('virtuosity/'))
            download(url, checked_path(root, name), expected)
    verify_sources(root, recipe)


def verify_sources(root, recipe):
    for name, expected in recipe['source_hashes'].items():
        if digest(checked_path(root, name)) != expected:
            raise ValueError(f'changed source: {name}')
    for voice in recipe['voices']:
        for sample in voice['samples']:
            if sample['source'] not in recipe['source_hashes']:
                raise ValueError('unlocked source')


def synthesized(voice, layer, rr):
    """Original deterministic percussion; no sampled or third-party waveforms."""
    rng = np.random.default_rng(voice['note'] * 1000 + layer * 10 + rr)
    t = np.arange(round(voice['tail'] * RATE)) / RATE
    noise = rng.standard_normal((len(t), 2))
    if voice['id'] == 'clap':
        env = sum(np.where(t >= s, np.exp(-np.maximum(t-s, 0)/.018), 0)
                  for s in [0, .011, .024, .039])
        y = signal.sosfilt(signal.butter(2, [800, 9000], 'bandpass', fs=RATE, output='sos'), noise, axis=0)
        y *= (env + .4*np.exp(-t/.12))[:, None]
    else:
        frequencies = {'splash': [3241, 4357, 5831, 7207, 8933, 10429],
                       'tambourine': [3203, 4289, 6113, 7817, 9209],
                       'cowbell': [540, 800, 1627]}[voice['id']]
        y = np.zeros((len(t), 2))
        for i, freq in enumerate(frequencies):
            decay = voice['tail'] / (5 + i)
            for ch in range(2):
                y[:, ch] += np.sin(2*np.pi*freq*(1+rr*.0008)*t + ch*.13*i) * np.exp(-t/decay) / len(frequencies)
        if voice['id'] != 'cowbell':
            n = signal.sosfilt(signal.butter(2, 3000, 'highpass', fs=RATE, output='sos'), noise, axis=0)
            y += .12*n*np.exp(-t/(voice['tail']/4))[:, None]
    return y * [.48, .74, 1.0][layer]


def prepare_audio(audio, rate, voice):
    if audio.ndim != 2 or audio.shape[1] not in (1, 2) or not np.isfinite(audio).all():
        raise ValueError('invalid source audio')
    if audio.shape[1] == 1:
        audio = np.repeat(audio, 2, axis=1)
    divisor = math.gcd(rate, RATE)
    audio = signal.resample_poly(audio, RATE//divisor, rate//divisor, axis=0)
    pitch = voice.get('pitch', 0)
    if pitch:
        ratio = 2**(pitch/12)
        audio = signal.resample_poly(audio, 10000, round(10000*ratio), axis=0)
    # Preserve 1 ms before the detected onset; remove only preceding silence.
    peak = float(np.max(np.abs(audio)))
    if peak < 1e-7:
        raise ValueError('silent source')
    active = np.flatnonzero(np.max(np.abs(audio), axis=1) > max(peak*.002, 1e-6))
    audio = audio[max(0, int(active[0])-48):][:round(voice['tail']*RATE)].copy()
    highpass = 25 if voice['kind'] == 'drum' else (100 if voice['kind'] == 'bell' else 180)
    audio = signal.sosfilt(signal.butter(2, highpass, 'highpass', fs=RATE, output='sos'), audio, axis=0)
    return audio


def process_voice(arrays, voice):
    # One reference gain for the complete voice preserves layer/RR dynamics.
    peak = max(float(np.max(np.abs(x))) for x in arrays)
    arrays = [x * (.8 / peak) for x in arrays]
    processed = []
    for x in arrays:
        t = np.arange(len(x)) / RATE
        if voice['id'].startswith('kick'):
            phase = 2*np.pi*(49*t + 70*.012*(1-np.exp(-t/.012)))
            x = x + (.13*np.max(np.abs(x))*np.sin(phase)*np.exp(-t/.12))[:, None]
        if voice['kind'] == 'drum' or voice['id'] == 'clap':
            # Linked RMS compression, 8:1 above -22 dBFS. The dry strike survives.
            alpha = math.exp(-1/(RATE*.025))
            rms = np.sqrt(signal.lfilter([1-alpha], [1, -alpha], np.mean(x*x, axis=1)) + 1e-12)
            over_db = np.maximum(0, 20*np.log10(rms) + 22)
            gain = 10**((-over_db*(1-1/8))/20)
            gain = signal.lfilter([1-alpha], [1, -alpha], gain, zi=[alpha])[0]
            wet = x * gain[:, None] * 10**(9/20)
            blend = .58 * np.minimum(t/.012, 1)
            x = x*(1-blend[:, None]) + wet*blend[:, None]
            x = np.tanh(x*1.4)/1.4
        else:
            # Cymbal transients and decays retain their dynamics; tame only peaks.
            x = np.tanh(x*1.08)/1.08
        fade = min(len(x)//2, 480 if voice['kind'] == 'drum' else 1920)
        x[-fade:] *= np.linspace(1, 0, fade)[:, None]
        x[:12] *= np.linspace(0, 1, 12)[:, None]
        processed.append(x)
    ceiling = 10**(-3/20)
    post_peak = max(float(np.max(np.abs(x))) for x in processed)
    return [x * min(1, ceiling/post_peak) for x in processed]


def build(root, output, recipe):
    if output.exists() or output.suffix != '.shrkit':
        raise ValueError('output must be a new .shrkit directory')
    verify_sources(root, recipe)
    output.mkdir(parents=True)
    try:
        (output/'samples').mkdir()
        manifest = dict(format_version=1, kit_id='power-studio', display_name='Power Studio',
            metadata=dict(author='SHR Drums; Lars Muldjord; FreePats; Versilian Studios and Karoryfer Samples',
                source='MuldjordKit FreePats 2020-10-18; Virtuosity Drums '+recipe['virtuosity_commit'],
                licence='CC-BY-4.0 AND CC0-1.0',
                attribution='Muldjord recordings: Lars Muldjord; stereo assembly: roberto@zenvoid.org. Virtuosity: Versilian Studios and Karoryfer Samples; drummer Austin McMahon. See CREDITS.md and PROVENANCE.json.',
                modification_notes='SHR Drums Power Studio 1.0: selected layers/round robins; resampled 48 kHz stereo PCM24; onset/tail trims; filtering; linked parallel compression and saturation on drums; mild cymbal saturation; generated kick reinforcement and original clap/tambourine/splash/cowbell. Pitched Tom4 (-2) and Tom1 (+2) variants; truncated choked-crash variant. No endorsement implied.',
                source_hashes=recipe['source_hashes']),
            engine=dict(minimum='0.2.1', maximum_exclusive='1.0.0'), max_polyphony=64,
            max_tail_seconds=8., articulations={}, voices=[],
            processing=dict(high_pass_hz=18., low_pass_hz=19000., saturation=.14, transient=.18,
                body=.12, parallel_compression=.38, room_amount=0., room_decay=.2,
                output_gain_db=-3., ceiling_dbfs=-1.5))
        provenance=[]
        for v in recipe['voices']:
            inputs=v['samples'] or [dict(source='original-synthesis',low=lo,high=hi,rr=rr)
                for lo,hi in [(1,42),(43,85),(86,127)] for rr in [1,2]]
            arrays=[]
            for i,s in enumerate(inputs):
                if s['source']=='original-synthesis':
                    x=synthesized(v,i//2,s['rr']); rate=RATE
                else:
                    x,rate=sf.read(checked_path(root,s['source']),dtype='float64',always_2d=True)
                arrays.append(prepare_audio(x,rate,v))
            arrays=process_voice(arrays,v)
            samples=[]
            for s,x in zip(inputs,arrays):
                name=f"samples/{v['note']:03}-{v['id']}-v{s['low']:03}-rr{s['rr']}.wav"
                if not np.isfinite(x).all():raise ValueError('nonfinite processed sample')
                sf.write(output/name,x,RATE,subtype='PCM_24')
                samples.append(dict(path=name,velocity_min=s['low'],velocity_max=s['high'],round_robin=s['rr'],
                    articulation=v['id'],gain_db=0.,metadata=dict(sample_rate=RATE,channels=2,frames=len(x)),sha256=digest(output/name)))
                provenance.append(dict(output=name,source=s['source'],source_sha256=recipe['source_hashes'].get(s['source']),
                    licence='CC-BY-4.0' if s['source'].startswith('muldjord/') else 'CC0-1.0',
                    peak=float(np.max(np.abs(x))),rms=float(np.sqrt(np.mean(x*x)))))
            family='kick' if v['id'].startswith('kick') else ('tom' if 'tom' in v['id'] else ('snare' if 'snare' in v['id'] or v['note']==37 else ('cymbal' if v['kind'] in ['hat','cymbal','bell'] or v['id']=='splash' else 'percussion')))
            # One-shot samples already contain their envelope; avoid a second decay.
            manifest['voices'].append(dict(id=v['id'],display_name=v['id'].replace('-',' ').title(),trigger_note=v['note'],
                articulation=v['id'],family=family,kind='sampled',choke_group=1 if v['kind']=='hat' else None,
                choke_release_ms=12.,gain_db=-7. if family in ['kick','snare','tom'] else -11.,pan=v['pan'],
                envelope=dict(attack_ms=.1,hold_ms=v['tail']*1000,decay_ms=100.,release_ms=v['tail']*1000),samples=samples,
                base_pitch_hz=None,tuning_limits=dict(down_cents=0,up_cents=0),follow_key=dict(type='excluded')))
            manifest['articulations'][v['id']]=v['note']
        decoded=sum(s['metadata']['frames']*8 for v in manifest['voices'] for s in v['samples'])
        if decoded>384*1024*1024:raise ValueError('decoded memory budget exceeded')
        (output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
        (output/'PROVENANCE.json').write_text(json.dumps(dict(recipe_sha256=digest(RECIPE),decoded_bytes=decoded,samples=provenance),indent=2)+'\n')
        for source,dest in [(MULD+'LICENSE.txt','LICENSE-CC-BY-4.0.txt'),(MULD+'README.txt','SOURCE-MULDJORD.txt'),('virtuosity/LICENSE','LICENSE-CC0-1.0.txt'),('virtuosity/README.md','SOURCE-VIRTUOSITY.md')]:
            shutil.copyfile(checked_path(root,source),output/dest)
        for name in ['CREDITS.md','README.md']:
            shutil.copyfile(RECIPE.parent/'power-studio'/name,output/name)
        print(f"Built {len(manifest['voices'])} voices, {len(provenance)} samples, {decoded} decoded bytes")
    except BaseException:
        shutil.rmtree(output)
        raise


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action',choices=['fetch','build'])
    parser.add_argument('--sources',type=Path,required=True)
    parser.add_argument('--output',type=Path)
    args=parser.parse_args()
    recipe=json.loads(RECIPE.read_text())
    if args.action=='fetch':fetch(args.sources,recipe)
    elif args.output is None:parser.error('build requires --output')
    else:build(args.sources,args.output,recipe)

if __name__=='__main__':main()

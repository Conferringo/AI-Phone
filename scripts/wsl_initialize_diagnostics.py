"""Closed WSL initialization evidence; no exception text, environment or raw logs."""
import os
import re

STEPS=frozenset(('not_started','preflight','source','tools','profile','engine_supervisor',
    'engine_ready','container_ownership','images','environment','database_start',
    'database_prepare','services_start','complete'))
ENGINE_STEPS=frozenset(('engine_supervisor','engine_ready'))
ERRORS=frozenset(('none','lock_busy','resource_missing','permission_denied','subprocess_timeout',
    'command_failed','validation_failed','os_error','unexpected'))
LIMIT=64*1024
SIGNALS={
    'executable_missing':(b'exec: not found',b'executable file not found'),
    'library_load_failure':(b'error loading shared library',b'error while loading shared libraries'),
    'permission_message':(b'permission denied',b'operation not permitted'),
    'cgroup_message':(b'cgroup',),
    'iptables_message':(b'iptables',b'nftables'),
    'storage_message':(b'failed to mount overlay',b'error initializing graphdriver'),
    'address_in_use':(b'address already in use',),
    'out_of_space':(b'no space left on device',),
}
DATABASE_SIGNALS={
    **{code:SIGNALS[code] for code in ('executable_missing','library_load_failure','cgroup_message')},
    'compose_unavailable':(b"is not a docker command",b"failed to fetch metadata"),
    'configuration_missing':(b'required variable',b'set postgres_password',b'no configuration file provided'),
    'configuration_invalid':(b'invalid interpolation format',b'validating '),
    'image_unavailable':(b'no such image',b'unable to find image'),
    'container_conflict':(b'is already in use by container',),
    'healthcheck_failed':(b'is unhealthy',),
    'container_exited':(b'exited (',),
    'container_runtime_failure':(b'failed to create task',b'oci runtime create failed'),
    'apparmor_message':(b'apparmor',),
    'seccomp_message':(b'seccomp',),
    'network_setup_failure':(b'failed to create network',b'failed to set up container networking'),
    'permission_message':(b'permission denied',b'operation not permitted'),
    'address_in_use':(b'address already in use',b'port is already allocated'),
    'out_of_space':(b'no space left on device',),
}
LOG_STATES=frozenset(('not_sampled','unavailable','unverified','empty','observed'))


def log_result(state='not_sampled'):
    return {'state':state,'flags':[],'bytes_read':0,'truncated':False}


def initial():
    return {'step':'not_started','error':'none','returncode':None,'engine':log_result(),'database':log_result()}


def database_report(output):
    """Tail of the outer fixture's already-open current-phase log; no path lookup."""
    try:
        output.flush();size=output.seek(0,os.SEEK_END)
        output.seek(max(0,size-LIMIT));body=output.read(min(size,LIMIT))
        lowered=body.lower()
        return {'state':'observed' if body else 'empty','bytes_read':len(body),'truncated':size>LIMIT,
                'flags':sorted(code for code,patterns in DATABASE_SIGNALS.items() if any(p in lowered for p in patterns))}
    except Exception:return log_result('unavailable')


def known_probe(value):
    return isinstance(value,str) and (value in PROBE_ERRORS or re.fullmatch(
        r'qualification_rpc_(?:registry/(?:state|device-save)|desktop/(?:provision|state|observe))_[1-5][0-9]{2}',value) is not None)


def checked_runtime_failure(value):
    if not isinstance(value,dict) or set(value)!={'error','returncode','probe'} or not isinstance(value['error'],str) or value['error'] not in ERRORS-{'none'}:
        raise ValueError('Invalid runtime failure evidence')
    _checked_returncode(value['returncode'],value['error'])
    if value['probe']!='unclassified' and not known_probe(value['probe']):raise ValueError('Invalid runtime probe code')


def checked(value,*,passed=False):
    _checked_initialization_fields(value)
    _checked_returncode(value['returncode'],value['error'])
    checked_log(value['engine'])
    checked_log(value['database'],DATABASE_SIGNALS)
    _checked_phase(value)
    _checked_completion(value['step'],value['error'],passed)


def _checked_initialization_fields(value):
    if not isinstance(value,dict) or set(value)!={'step','error','returncode','engine','database'}:
        raise ValueError('Invalid initialization evidence fields')
    if not isinstance(value['step'],str) or value['step'] not in STEPS or not isinstance(value['error'],str) or value['error'] not in ERRORS:
        raise ValueError('Invalid initialization evidence enum')


def _checked_returncode(number,error):
    if number is not None and (type(number) is not int or not -(2**31)<=number<2**31 or number==0 or error!='command_failed'):
        raise ValueError('Invalid initialization command status')


def _checked_phase(value):
    if value['database']['state']=='unverified':raise ValueError('Invalid database startup evidence state')
    if value['database']['state']!='not_sampled' and (value['step']!='database_start' or value['error']!='command_failed'):
        raise ValueError('Unexpected database startup evidence')
    if value['engine']['state']!='not_sampled' and (value['step'] not in ENGINE_STEPS or value['error']=='none'):
        raise ValueError('Unexpected engine evidence')


def _checked_completion(step,error,passed):
    if step=='complete' and error!='none':raise ValueError('Initialization completion conflicts with failure')
    if passed and (step!='complete' or error!='none'):
        raise ValueError('Incomplete initialization evidence')


def checked_log(value,signals=SIGNALS):
    _checked_log_fields(value,signals)
    _checked_log_flags(value['flags'],signals)
    _checked_log_budget(value['bytes_read'],value['truncated'])
    _checked_log_observation(value)


def _checked_log_fields(value,signals):
    if not isinstance(value,dict) or set(value)!={'state','flags','bytes_read','truncated'}:
        raise ValueError('Invalid engine evidence fields')
    state=value['state'];flags=value['flags']
    if not isinstance(state,str) or state not in LOG_STATES or not isinstance(flags,list) or len(flags)>len(signals):
        raise ValueError('Invalid engine evidence enum')


def _checked_log_flags(flags,signals):
    if any(not isinstance(flag,str) or flag not in signals for flag in flags) or flags!=sorted(set(flags)):
        raise ValueError('Invalid engine evidence flags')


def _checked_log_budget(size,truncated):
    if type(size) is not int or not 0<=size<=LIMIT or type(truncated) is not bool:
        raise ValueError('Invalid engine evidence size')


def _checked_log_observation(value):
    size=value['bytes_read'];flags=value['flags']
    if value['state']=='observed':
        if size==0 or (value['truncated'] and size!=LIMIT):raise ValueError('Invalid observed engine evidence')
    elif size or flags or value['truncated']:raise ValueError('Unexpected engine evidence content')

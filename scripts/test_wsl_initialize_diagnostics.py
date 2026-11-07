"""Synthetic files and mocked processes only; never starts a host engine or device."""
import os
import unittest

if os.name!='posix':
    raise unittest.SkipTest('WSL initialization owner fixtures require POSIX')

import subprocess
from copy import deepcopy
from unittest.mock import patch

import wsl_initialize_diagnostics as diag
import wsl_runtime_fixture as fixture


def receipt():
    return {'format':1,'nonce':'a'*32,'phase':1,'status':'failed','stage':'initialize',
        'checks':{name:False for name in fixture.CHECKS},'frame':{},'windows_boot_verified':False,
        'phone_ready':False,'initialization':diag.initial()}


class NoHostProcesses(unittest.TestCase):
    def setUp(self):
        for name in ('run','Popen'):
            guard=patch.object(subprocess,name,side_effect=AssertionError('Real host process forbidden'))
            guard.start();self.addCleanup(guard.stop)


class ErrorAndSchemaTests(NoHostProcesses):
    def test_closed_receipt_shapes_reject_injected_values_and_false_success(self):
        value=receipt();fixture.worker_result(value,'a'*32,1)
        changes=[('step',[]),('step','private'),('error',{}),('error','private'),('returncode',True),('returncode',1),
            ('engine',[]),('engine',{'state':'private'}),('private','secret')]
        for key,bad in changes:
            changed=deepcopy(value);changed['initialization'][key]=bad
            with self.assertRaises(ValueError):fixture.worker_result(changed,'a'*32,1)
        for patch_value in ({'state':[]},{'state':'private'},{'flags':['private']},{'flags':[[]]},
                {'flags':['cgroup_message','cgroup_message']},{'bytes_read':True},{'bytes_read':diag.LIMIT+1},
                {'truncated':1},{'private':'secret'},{'state':'observed'},{'flags':['cgroup_message']}):
            changed=deepcopy(value);changed['initialization']['engine'].update(patch_value)
            with self.assertRaises(ValueError):fixture.worker_result(changed,'a'*32,1)
        changed=deepcopy(value);changed.update(status='passed',stage='stopped',checks={name:True for name in fixture.CHECKS},frame={'width':1,'height':1,'sha256':'b'*64})
        with self.assertRaises(ValueError):fixture.worker_result(changed,'a'*32,1)
        changed['initialization']['step']='complete';fixture.worker_result(changed,'a'*32,1)
        changed['initialization']['error']='unexpected'
        with self.assertRaises(ValueError):fixture.worker_result(changed,'a'*32,1)

    def test_log_shape_and_phase_binding_are_strict(self):
        value=receipt();value['initialization'].update(step='engine_ready',error='command_failed',returncode=-9,
            engine={'state':'observed','flags':['cgroup_message'],'bytes_read':diag.LIMIT,'truncated':True})
        fixture.worker_result(value,'a'*32,1)
        for section,changes in [('initialization',{'step':'images'}),('initialization',{'error':'none'}),
                ('initialization',{'returncode':0}),('engine',{'bytes_read':1}),('engine',{'flags':list(diag.SIGNALS)})]:
            changed=deepcopy(value);target=changed['initialization'] if section=='initialization' else changed['initialization']['engine'];target.update(changes)
            with self.assertRaises(ValueError):fixture.worker_result(changed,'a'*32,1)
        for stage in ('guard','nesting','readiness','desktop','stopped'):
            changed=deepcopy(value);changed['stage']=stage
            with self.assertRaises(ValueError):fixture.worker_result(changed,'a'*32,1)


if __name__=='__main__':unittest.main()

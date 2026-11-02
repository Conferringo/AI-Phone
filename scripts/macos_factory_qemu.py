"""Maintainer-only TCG adapter; never used by the installed client's VZ engine."""
import os
from pathlib import Path
import re
import sys


def software_arguments(arguments):
    result=list(arguments)
    for index,argument in enumerate(result[:-1]):
        if argument in ('-machine','-M'):
            result[index+1]=re.sub(r'(^|,)accel=[^,]+',r'\1accel=tcg',result[index+1])
        elif argument=='-accel' and result[index+1]!='help':
            result[index+1]='tcg,thread=multi'
    return result


def main():
    executable=Path(os.environ['AI_PHONE_FACTORY_QEMU']).resolve(strict=True)
    os.execv(executable,[str(executable),*software_arguments(sys.argv[1:])])


if __name__=='__main__':main()

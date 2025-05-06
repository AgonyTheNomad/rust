#!/usr/bin/env python3
"""
Command handler for the Hyperliquid trading bot.
Processes command files to control the bot's behavior.
"""

import json
import logging
from pathlib import Path

logger = logging.getLogger("hyperliquid_trader")

class CommandHandler:
    """
    Handles command files used to control the trader's behavior.
    Commands can be used to stop, pause, resume, or reconfigure the trader.
    """
    
    def __init__(self, commands_dir, archive_dir, trader, config_path):
        """
        Initialize the command handler.
        
        Parameters:
        -----------
        commands_dir : str or Path
            Directory where command files are placed
        archive_dir : str or Path
            Directory where processed command files are archived
        trader : object
            Reference to the main trader object, used to control its behavior
        config_path : str
            Path to the configuration file
        """
        self.commands_dir = Path(commands_dir)
        self.archive_dir = Path(archive_dir)
        self.trader = trader
        self.config_path = config_path
        
        # Create directories if they don't exist
        self.commands_dir.mkdir(exist_ok=True)
        
        logger.info(f"Command handler initialized with directory: {commands_dir}")
    
    async def check_commands(self):
        """
        Check for command files and process them.
        Command files are JSON files with a .cmd extension.
        """
        for cmd_file in self.commands_dir.glob('*.cmd'):
            try:
                with open(cmd_file, 'r') as f:
                    command = json.load(f)
                
                cmd_type = command.get('type')
                logger.info(f"Processing command: {cmd_type}")
                
                # Process the command based on its type
                processed = await self.process_command(cmd_type, command)
                
                # Archive the command file if it was processed
                if processed:
                    target = self.archive_dir / cmd_file.name
                    cmd_file.rename(target)
                    logger.info(f"Archived command file: {cmd_file.name}")
                else:
                    logger.warning(f"Command {cmd_type} was not processed successfully")
                
            except Exception as e:
                logger.error(f"Error processing command file {cmd_file}: {e}")
    
    async def process_command(self, cmd_type, command):
        """
        Process a command based on its type.
        
        Parameters:
        -----------
        cmd_type : str
            Type of command to process
        command : dict
            Command data
        
        Returns:
        --------
        bool
            True if the command was processed successfully, False otherwise
        """
        if cmd_type == 'stop':
            logger.info("Received stop command. Exiting...")
            exit(0)
            return True
        
        elif cmd_type == 'pause':
            logger.info("Pausing trading")
            self.trader.set_paused(True)
            return True
        
        elif cmd_type == 'resume':
            logger.info("Resuming trading")
            self.trader.set_paused(False)
            return True
        
        elif cmd_type == 'config':
            # Update configuration
            params = command.get('params', {})
            key = params.get('key')
            value = params.get('value')
            
            if key and value is not None:
                success = self.trader.update_config(key, value)
                return success
            else:
                logger.warning(f"Invalid config command parameters: {params}")
                return False
        
        elif cmd_type == 'cancel_all':
            # Cancel all open orders
            try:
                logger.info("Canceling all open orders")
                # This would need to be implemented in the trader or position manager
                # For now, just log it
                logger.warning("Cancel all orders functionality not implemented yet")
                return True
            except Exception as e:
                logger.error(f"Error canceling all orders: {e}")
                return False
        
        elif cmd_type == 'cancel_order':
            # Cancel a specific order
            order_id = command.get('order_id')
            if order_id:
                try:
                    logger.info(f"Canceling order: {order_id}")
                    # This would need to be implemented in the trader or position manager
                    # For now, just log it
                    logger.warning("Cancel specific order functionality not implemented yet")
                    return True
                except Exception as e:
                    logger.error(f"Error canceling order {order_id}: {e}")
                    return False
            else:
                logger.warning("Cancel order command missing order_id")
                return False
        
        else:
            logger.warning(f"Unknown command type: {cmd_type}")
            return True  # Still mark as processed even if unknown